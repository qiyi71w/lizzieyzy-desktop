//! Java `NvidiaGpuDetector` and `AppleSiliconHardwareProbe`: bounded, non-identifying probes.
use serde::{Deserialize, Serialize};
use std::{
    collections::BTreeMap,
    fs,
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::mpsc,
    thread,
    time::{Duration, Instant},
};

#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum CudaCompatibility {
    Supported,
    ProbeRequired,
    Unsupported,
    #[default]
    Unknown,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub enum NvidiaQueryStatus {
    Found,
    None,
    #[default]
    Failed,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct ComputeCapability {
    pub major: u32,
    pub minor: u32,
    pub inferred: bool,
}
impl ComputeCapability {
    pub fn rank(&self) -> u32 {
        self.major * 10 + self.minor
    }
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct NvidiaGpu {
    pub name: String,
    pub compute: Option<ComputeCapability>,
    pub driver: Option<String>,
    pub memory_mib: Option<u64>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct AppleFacts {
    pub model: Option<String>,
    pub chip: Option<String>,
    pub build: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, ts_rs::TS)]
#[serde(rename_all = "camelCase")]
pub struct MachineFacts {
    pub os: String,
    pub arch: String,
    pub rosetta: Option<bool>,
    pub logical_cpus: Option<u32>,
    pub total_memory_bytes: Option<u64>,
    pub cpu_name: Option<String>,
    pub cpu_avx2_fma: Option<bool>,
    pub fuse: Option<bool>,
    pub library_versions: BTreeMap<String, String>,
    pub nvidia_gpus: Vec<NvidiaGpu>,
    pub nvidia_status: NvidiaQueryStatus,
    pub cuda_compatibility: CudaCompatibility,
    pub apple: Option<AppleFacts>,
    /// None means the cache/ICD probe failed, not that libraries are absent.
    pub loader_libraries: Option<Vec<String>>,
    pub opencl_icd: Option<bool>,
}
impl MachineFacts {
    pub fn best_gpu(&self) -> Option<&NvidiaGpu> {
        best_gpu(&self.nvidia_gpus)
    }
    pub fn platform(&self) -> Option<&'static str> {
        match (self.os.as_str(), self.arch.as_str()) {
            ("windows", "x86_64" | "amd64" | "x64") => Some("windows-x64"),
            ("linux", "x86_64" | "amd64" | "x64") => Some("linux-x64"),
            ("macos", _) if self.rosetta == Some(true) => Some("macos-arm64"),
            ("macos", "aarch64" | "arm64") => Some("macos-arm64"),
            ("macos", "x86_64" | "amd64" | "x64") => Some("macos-amd64"),
            _ => None,
        }
    }
}
#[derive(Clone, Debug, Default)]
pub struct ProbeOutput {
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub timed_out: bool,
}
impl ProbeOutput {
    pub fn success(&self) -> bool {
        !self.timed_out && self.exit_code == Some(0)
    }
    pub fn combined(&self) -> String {
        format!("{}\n{}", self.stdout, self.stderr).trim().to_string()
    }
}
/// The operating-system seam, shared by discovery and explicit verification.
pub trait Probe {
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> io::Result<ProbeOutput>;
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>>;
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>>;
    fn env_var(&self, key: &str) -> Option<String>;
    fn cpu_avx2_fma(&self) -> Option<bool> {
        None
    }
    fn logical_cpus(&self) -> Option<u32> {
        None
    }
    /// Target of a symbolic link; an error means the version behind the path is unknown.
    fn read_link(&self, _path: &Path) -> io::Result<PathBuf> {
        Err(io::Error::from(io::ErrorKind::Unsupported))
    }
}
#[derive(Clone, Debug, Default)]
pub struct StdProbe {
    pub library_dirs: Vec<PathBuf>,
}
impl StdProbe {
    pub fn with_library_dirs(library_dirs: Vec<PathBuf>) -> Self {
        Self { library_dirs }
    }
}
impl Probe for StdProbe {
    fn cpu_avx2_fma(&self) -> Option<bool> {
        #[cfg(target_arch = "x86_64")]
        {
            Some(std::arch::is_x86_feature_detected!("avx2") && std::arch::is_x86_feature_detected!("fma"))
        }
        #[cfg(not(target_arch = "x86_64"))]
        {
            None
        }
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        fs::read_link(path)
    }
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> io::Result<ProbeOutput> {
        let path = Path::new(program);
        let program_path = if path.components().count() > 1 {
            fs::canonicalize(path)?
        } else {
            path.to_path_buf()
        };
        let mut command = Command::new(&program_path);
        if let Some(parent) = program_path.parent().filter(|p| !p.as_os_str().is_empty()) {
            command.current_dir(parent);
        }
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
            if !self.library_dirs.is_empty() {
                let mut paths = self.library_dirs.clone();
                if let Some(path) = std::env::var_os("PATH") {
                    paths.extend(std::env::split_paths(&path));
                }
                let path = std::env::join_paths(paths).map_err(|e| io::Error::new(io::ErrorKind::InvalidInput, e))?;
                command.env("PATH", path);
            }
        }
        let mut child = command.spawn()?;
        let stdout = child.stdout.take().expect("piped stdout");
        let stderr = child.stderr.take().expect("piped stderr");
        let (tx, rx) = mpsc::channel();
        for (mut stream, is_err) in [
            (Box::new(stdout) as Box<dyn Read + Send>, false),
            (Box::new(stderr) as Box<dyn Read + Send>, true),
        ] {
            let tx = tx.clone();
            thread::spawn(move || {
                let mut bytes = Vec::new();
                // Continue draining after the diagnostic cap to avoid blocking the child.
                let mut buffer = [0; 8192];
                while let Ok(n) = stream.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    let keep = n.min((4 * 1024 * 1024_usize).saturating_sub(bytes.len()));
                    bytes.extend_from_slice(&buffer[..keep]);
                }
                let _ = tx.send((is_err, String::from_utf8_lossy(&bytes).into_owned()));
            });
        }
        drop(tx);
        let started = Instant::now();
        let (status, timed_out) = loop {
            if let Some(status) = child.try_wait()? {
                break (status, false);
            }
            if started.elapsed() >= timeout {
                let _ = child.kill();
                break (child.wait()?, true);
            }
            thread::sleep(Duration::from_millis(10));
        };
        let mut result = ProbeOutput {
            exit_code: status.code(),
            timed_out,
            ..Default::default()
        };
        for _ in 0..2 {
            if let Ok((is_err, output)) = rx.recv_timeout(Duration::from_secs(1)) {
                if is_err {
                    result.stderr = output;
                } else {
                    result.stdout = output;
                }
            }
        }
        Ok(result)
    }
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        fs::read(path)
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        fs::read_dir(path)?.map(|e| e.map(|e| e.path())).collect()
    }
    fn env_var(&self, key: &str) -> Option<String> {
        std::env::var(key).ok()
    }
    fn logical_cpus(&self) -> Option<u32> {
        thread::available_parallelism()
            .ok()
            .and_then(|n| u32::try_from(n.get()).ok())
    }
}

fn known(value: &str) -> Option<String> {
    let value = value.trim();
    if value.is_empty()
        || value.eq_ignore_ascii_case("N/A")
        || value.eq_ignore_ascii_case("unknown")
        || value.contains("Not Supported")
    {
        None
    } else {
        Some(value.to_string())
    }
}
fn csv(line: &str) -> Vec<String> {
    let mut fields = Vec::new();
    let mut field = String::new();
    let mut quoted = false;
    for c in line.chars() {
        match c {
            '"' => quoted = !quoted,
            ',' if !quoted => {
                fields.push(std::mem::take(&mut field));
            }
            _ => field.push(c),
        }
    }
    fields.push(field);
    fields
}
/// Java `parseGpuRows`: quoted CSV, optional compute_cap, N/A remains unknown.
pub fn parse_nvidia_smi(output: &str, includes_compute: bool) -> Vec<NvidiaGpu> {
    output
        .lines()
        .filter_map(|line| {
            let fields = csv(line);
            if fields.len() < if includes_compute { 4 } else { 3 } {
                return None;
            }
            let name = known(&fields[0])?;
            let compute = if includes_compute {
                parse_compute(&fields[1])
            } else {
                None
            }
            .or_else(|| infer_compute(&name));
            let driver = known(&fields[if includes_compute { 2 } else { 1 }]);
            let memory_text = fields[if includes_compute { 3 } else { 2 }]
                .replace("MiB", "")
                .replace("MB", "");
            let memory_mib = memory_text.trim().parse::<u64>().ok().filter(|n| *n > 0);
            Some(NvidiaGpu {
                name,
                compute,
                driver,
                memory_mib,
            })
        })
        .collect()
}
fn parse_compute(text: &str) -> Option<ComputeCapability> {
    let clean = text.trim().strip_prefix("Compute Capability").unwrap_or(text).trim();
    let (major, minor) = clean.split_once('.')?;
    let major = major.trim().parse().ok()?;
    let minor = minor.trim().parse().ok()?;
    if major > 0 {
        Some(ComputeCapability {
            major,
            minor,
            inferred: false,
        })
    } else {
        None
    }
}
/// Java `inferComputeCapabilityFromName` (including workstation/datacenter names).
pub fn infer_compute(name: &str) -> Option<ComputeCapability> {
    let upper = name.to_uppercase();
    let words: Vec<&str> = upper
        .split(|c: char| !c.is_ascii_alphanumeric())
        .filter(|s| !s.is_empty())
        .collect();
    let has = |word: &str| words.contains(&word);
    let generation = |prefix: &str| -> Option<u32> {
        let pos = words.iter().position(|w| *w == prefix)?;
        let digits = words.get(pos + 1)?;
        if digits.len() != 4 {
            return None;
        }
        digits[..2].parse().ok()
    };
    let pair = if upper.contains("BLACKWELL") || upper.contains("RTX PRO") {
        (12, 0)
    } else if has("ADA") {
        (8, 9)
    } else if let Some(generation) = generation("RTX") {
        match generation {
            50.. => (12, 0),
            40..=49 => (8, 9),
            30..=39 => (8, 6),
            20..=29 => (7, 5),
            _ => return None,
        }
    } else if let Some(generation) = generation("GTX") {
        match generation {
            16 => (7, 5),
            10 => (6, 1),
            9 => (5, 2),
            _ => return None,
        }
    } else if upper.contains("TITAN RTX") || upper.contains("QUADRO RTX") || has("T4") {
        (7, 5)
    } else if upper.contains("RTX A") || ["A40", "A10", "A16", "A2"].iter().any(|w| has(w)) {
        (8, 6)
    } else if has("A100") {
        (8, 0)
    } else if ["L4", "L40", "L40S"].iter().any(|w| has(w)) {
        (8, 9)
    } else {
        return None;
    };
    Some(ComputeCapability {
        major: pair.0,
        minor: pair.1,
        inferred: true,
    })
}
pub fn best_gpu(gpus: &[NvidiaGpu]) -> Option<&NvidiaGpu> {
    gpus.iter().reduce(|best, candidate| {
        let rank = |g: &NvidiaGpu| {
            (
                g.compute.as_ref().map_or(0, ComputeCapability::rank),
                g.memory_mib.unwrap_or(0),
            )
        };
        if rank(candidate) > rank(best) {
            candidate
        } else {
            best
        }
    })
}
pub fn cuda_compatibility(driver: Option<&str>) -> CudaCompatibility {
    let Some(driver) = driver else {
        return CudaCompatibility::Unknown;
    };
    static DRIVER_PATTERN: std::sync::LazyLock<fancy_regex::Regex> =
        std::sync::LazyLock::new(|| fancy_regex::Regex::new(r"(\d+)(?:[.](\d+))?").expect("driver pattern"));
    let normalized = driver.replace(',', ".");
    let Some(captures) = DRIVER_PATTERN.captures(&normalized).ok().flatten() else {
        return CudaCompatibility::Unknown;
    };
    let Some(major) = captures
        .get(1)
        .and_then(|s| s.as_str().parse::<u32>().ok())
        .filter(|n| *n > 0)
    else {
        return CudaCompatibility::Unknown;
    };
    let minor = captures
        .get(2)
        .and_then(|s| s.as_str().parse::<u32>().ok())
        .unwrap_or(0);
    if (major, minor) >= (570, 65) {
        CudaCompatibility::Supported
    } else if (major, minor) >= (528, 33) {
        CudaCompatibility::ProbeRequired
    } else {
        CudaCompatibility::Unsupported
    }
}
fn run_value(probe: &dyn Probe, program: &str, args: &[&str], seconds: u64) -> Option<String> {
    let args: Vec<String> = args.iter().map(|s| (*s).into()).collect();
    probe
        .run(program, &args, Duration::from_secs(seconds))
        .ok()
        .filter(ProbeOutput::success)
        .and_then(|o| known(o.stdout.lines().next().unwrap_or("")))
}
fn sysctl(probe: &dyn Probe, key: &str) -> Option<String> {
    run_value(probe, "/usr/sbin/sysctl", &["-n", key], 3).and_then(|s| {
        known(
            s.strip_prefix(&format!("{key}: "))
                .or_else(|| s.strip_prefix(&format!("{key}=")))
                .unwrap_or(&s),
        )
    })
}
fn positive<T: std::str::FromStr + PartialOrd + Default>(value: Option<String>) -> Option<T> {
    value?.parse::<T>().ok().filter(|n| *n > T::default())
}
/// `os` and `arch` are explicit to make every platform path testable on any host.
pub fn detect_machine(probe: &dyn Probe, os: &str, arch: &str) -> MachineFacts {
    let mut facts = MachineFacts {
        os: os.into(),
        arch: arch.into(),
        logical_cpus: probe.logical_cpus(),
        ..Default::default()
    };
    if matches!(arch, "x86_64" | "amd64" | "x64") {
        facts.cpu_avx2_fma = probe.cpu_avx2_fma();
    }
    let mut programs = Vec::new();
    if os == "windows" {
        if let Some(root) = probe.env_var("SystemRoot") {
            programs.push(format!(
                "{}\\System32\\nvidia-smi.exe",
                root.trim_end_matches(['/', '\\'])
            ));
        }
        facts.cpu_name = probe.env_var("PROCESSOR_IDENTIFIER").and_then(|v| known(&v));
        // Available on supported Windows releases; failure is unknown, not zero.
        facts.total_memory_bytes = positive(run_value(
            probe,
            "powershell.exe",
            &[
                "-NoProfile",
                "-NonInteractive",
                "-Command",
                "(Get-CimInstance Win32_ComputerSystem).TotalPhysicalMemory",
            ],
            3,
        ));
    }
    programs.push("nvidia-smi".into());
    if os == "linux" {
        programs.push("/usr/lib/wsl/lib/nvidia-smi".into());
    }
    let mut succeeded = false;
    'query: for include in [true, false] {
        let fields = if include {
            "--query-gpu=name,compute_cap,driver_version,memory.total"
        } else {
            "--query-gpu=name,driver_version,memory.total"
        };
        for program in &programs {
            if let Ok(result) = probe.run(
                program,
                &[fields.into(), "--format=csv,noheader,nounits".into()],
                Duration::from_secs(8),
            ) {
                if result.success() {
                    succeeded = true;
                    let gpus = parse_nvidia_smi(&result.stdout, include);
                    if !gpus.is_empty() {
                        facts.nvidia_gpus = gpus;
                        break 'query;
                    }
                }
                if !result.timed_out && result.combined().to_lowercase().contains("no devices were found") {
                    succeeded = true;
                }
            }
        }
    }
    facts.nvidia_status = if !facts.nvidia_gpus.is_empty() {
        NvidiaQueryStatus::Found
    } else if succeeded {
        NvidiaQueryStatus::None
    } else {
        NvidiaQueryStatus::Failed
    };
    facts.cuda_compatibility = cuda_compatibility(facts.best_gpu().and_then(|g| g.driver.as_deref()));
    if os == "macos" {
        let model = sysctl(probe, "hw.model");
        let chip = sysctl(probe, "machdep.cpu.brand_string").or_else(|| model.clone());
        facts.cpu_name.clone_from(&chip);
        facts.logical_cpus = positive(sysctl(probe, "hw.logicalcpu")).or(facts.logical_cpus);
        facts.total_memory_bytes = positive(sysctl(probe, "hw.memsize"));
        facts.rosetta = sysctl(probe, "sysctl.proc_translated").and_then(|s| match s.as_str() {
            "1" => Some(true),
            "0" => Some(false),
            _ => None,
        });
        if let Some(machine) = sysctl(probe, "hw.machine") {
            facts.arch = machine;
        }
        facts.apple = Some(AppleFacts {
            model,
            chip,
            build: run_value(probe, "/usr/bin/sw_vers", &["-buildVersion"], 3),
        });
    } else if os == "linux" {
        facts.fuse = detect_fuse(probe);
        if let Ok(bytes) = probe.read_file(Path::new("/proc/meminfo")) {
            let text = String::from_utf8_lossy(&bytes);
            facts.total_memory_bytes = text
                .lines()
                .find_map(|l| {
                    l.strip_prefix("MemTotal:")
                        .and_then(|s| s.split_whitespace().next())
                        .and_then(|s| s.parse::<u64>().ok())
                        .and_then(|n| n.checked_mul(1024))
                })
                .filter(|n| *n > 0);
        }
        if let Ok(bytes) = probe.read_file(Path::new("/proc/cpuinfo")) {
            facts.cpu_name = String::from_utf8_lossy(&bytes).lines().find_map(|l| {
                l.split_once(':')
                    .filter(|(k, _)| k.trim() == "model name")
                    .and_then(|(_, v)| known(v))
            });
        }
        for program in ["/sbin/ldconfig", "ldconfig"] {
            if let Ok(output) = probe.run(program, &["-p".into()], Duration::from_secs(3)) {
                if output.success() {
                    facts.loader_libraries = Some(parse_loader_cache(&output.stdout));
                    facts.library_versions = detect_library_versions(probe, &output.stdout);
                    break;
                }
            }
        }
        let icds = probe.list_dir(Path::new("/etc/OpenCL/vendors"));
        facts.opencl_icd = match (&facts.loader_libraries, icds) {
            (Some(libs), Ok(paths)) => Some(
                libs.iter().any(|l| l == "libOpenCL.so.1")
                    && paths.iter().any(|p| p.extension().is_some_and(|e| e == "icd")),
            ),
            (_, Err(e)) if e.kind() == io::ErrorKind::NotFound => Some(false),
            (Some(libs), _) if !libs.iter().any(|l| l == "libOpenCL.so.1") => Some(false),
            _ => None,
        };
    } else if os == "windows" {
        facts.opencl_icd = probe.env_var("SystemRoot").and_then(|root| {
            match probe.read_file(Path::new(&format!("{root}\\System32\\OpenCL.dll"))) {
                Ok(_) => Some(true),
                Err(e) if e.kind() == io::ErrorKind::NotFound => Some(false),
                Err(_) => None,
            }
        });
    }
    facts
}
pub fn detect_current_machine() -> MachineFacts {
    detect_machine(&StdProbe::default(), std::env::consts::OS, std::env::consts::ARCH)
}

fn detect_fuse(probe: &dyn Probe) -> Option<bool> {
    let devices = probe.list_dir(Path::new("/dev")).ok()?;
    if !devices.iter().any(|p| p.file_name().is_some_and(|n| n == "fuse")) {
        return Some(false);
    }
    let path = probe.env_var("PATH")?;
    let mut unknown = false;
    for directory in path.split(':').filter(|d| !d.is_empty()) {
        match probe.list_dir(Path::new(directory)) {
            Ok(entries)
                if entries
                    .iter()
                    .any(|p| p.file_name().is_some_and(|n| n == "fusermount" || n == "fusermount3")) =>
            {
                return Some(true)
            }
            Err(e) if e.kind() != io::ErrorKind::NotFound => unknown = true,
            _ => {}
        }
    }
    if unknown {
        None
    } else {
        Some(false)
    }
}

/// The version the loader will use: `ldconfig -p` lists entries in search order, so only the
/// first entry per soname counts, and its version comes from the soname link's target
/// (`libcudnn.so.9 -> libcudnn.so.9.8.0`). Anything not established that way stays unknown.
fn detect_library_versions(probe: &dyn Probe, output: &str) -> BTreeMap<String, String> {
    let mut versions = BTreeMap::new();
    let mut seen = std::collections::BTreeSet::new();
    for line in output.lines() {
        let Some((left, resolved)) = line.split_once("=>") else {
            continue;
        };
        let Some(soname) = left.split_whitespace().next() else {
            continue;
        };
        let Some(major) = ["libcublas.so.", "libcublasLt.so.", "libnvrtc.so.", "libcudnn.so."]
            .iter()
            .find_map(|prefix| soname.strip_prefix(prefix))
            .filter(|major| major.parse::<u32>().is_ok())
        else {
            continue;
        };
        if !seen.insert(soname) {
            continue;
        }
        let Ok(target) = probe.read_link(Path::new(resolved.trim())) else {
            continue;
        };
        let Some(suffix) = target
            .file_name()
            .and_then(|n| n.to_str())
            .and_then(|n| n.strip_prefix(soname))
            .and_then(|n| n.strip_prefix('.'))
        else {
            continue;
        };
        if !suffix.is_empty() && suffix.split('.').all(|n| n.parse::<u32>().is_ok()) {
            versions.insert(soname.to_owned(), format!("{major}.{suffix}"));
        }
    }
    versions
}
pub fn parse_loader_cache(output: &str) -> Vec<String> {
    let mut libraries: Vec<String> = output
        .lines()
        .filter(|l| l.contains(" => "))
        .filter_map(|l| l.split_whitespace().next().map(str::to_string))
        .collect();
    libraries.sort();
    libraries.dedup();
    libraries
}
