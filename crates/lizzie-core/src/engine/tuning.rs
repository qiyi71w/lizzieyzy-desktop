//! Profile-local performance policy (Java `EngineThreadPolicy`, `KataGoRuntimeHelper`).
use super::gtp::{self, LaunchSpec};
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use ts_rs::TS;

#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum ThreadSource {
    #[default]
    CFG,
    BENCHMARK,
}

/// Java `EngineData.threadPolicy`; Java keys are preserved for import and attribution.
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct ThreadPolicy {
    pub source: ThreadSource,
    pub source_revision: u64,
    pub initial_setup: bool,
    #[serde(rename = "katago-benchmark-threads")]
    pub threads: u32,
    #[serde(rename = "katago-benchmark-current-threads")]
    pub current_threads: u32,
    #[serde(rename = "katago-benchmark-backend")]
    pub backend: String,
    #[serde(rename = "katago-benchmark-summary")]
    pub summary: String,
    #[serde(rename = "katago-benchmark-updated-at")]
    pub updated_at: u64,
    #[serde(rename = "katago-benchmark-topology")]
    pub topology: String,
    #[serde(rename = "katago-benchmark-nn-server-threads")]
    pub nn_server_threads: u32,
    #[serde(rename = "katago-benchmark-batch-size")]
    pub batch_size: u32,
    #[serde(rename = "katago-benchmark-visits-per-second")]
    pub visits_per_second: f64,
    #[serde(rename = "katago-benchmark-nn-evals-per-second")]
    pub nn_evals_per_second: f64,
    #[serde(rename = "katago-benchmark-average-batch-size")]
    pub average_batch_size: f64,
    #[ts(type = "unknown")]
    pub environment: Option<Value>,
    #[serde(rename = "katago-tuning-profiles-v1", alias = "katago-apple-tuning-profile-v1")]
    #[ts(type = "unknown")]
    pub tuning_profiles: Option<Value>,
    #[serde(rename = "katago-measured-scenes-v1")]
    #[ts(type = "unknown")]
    pub measured_scenes: Option<Value>,
}
impl ThreadPolicy {
    pub fn initial_setup() -> Self {
        Self {
            initial_setup: true,
            ..Self::default()
        }
    }
    /// Java `threadsForLaunch`: CFG and explicit command overrides are never injected.
    pub fn threads_for_launch(&self, command: &str) -> u32 {
        if self.source == ThreadSource::CFG || !is_local_katago(command) || has_search_threads(command) {
            0
        } else {
            self.threads
        }
    }
}

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct GtpConfiguration {
    pub protocol: String,
    #[ts(type = "Record<string, unknown>")]
    pub profile: BTreeMap<String, Value>,
}

pub fn is_local_katago(command: &str) -> bool {
    gtp::split_command(command).first().is_some_and(|s| {
        let name = s.rsplit(['/', '\\']).next().unwrap_or("").to_ascii_lowercase();
        name.contains("katago") && !s.contains("://")
    })
}

/// Java `inspectSavedEngine`: `-option=value` is read as `-option value`.
pub fn split_option_values(args: &[String]) -> Vec<String> {
    args.iter()
        .flat_map(|token| match token.split_once('=') {
            Some((option, value)) if token.starts_with('-') && !option.is_empty() => {
                vec![option.to_string(), value.to_string()]
            }
            _ => vec![token.clone()],
        })
        .collect()
}

/// Effective `-override-config` assignments, last one wins (`KataGoCommandSpec`).
pub fn overrides(args: &[String]) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for pair in args.windows(2) {
        if matches!(pair[0].as_str(), "-override-config" | "--override-config") {
            for item in pair[1].split(',') {
                if let Some((k, v)) = item.split_once('=') {
                    out.insert(k.trim().to_ascii_lowercase(), v.trim().to_string());
                }
            }
        }
        if matches!(pair[0].as_str(), "-threads" | "--threads") {
            out.insert("numsearchthreads".into(), pair[1].clone());
        }
    }
    out
}
pub fn has_search_threads(command: &str) -> bool {
    overrides(&gtp::split_command(command))
        .get("numsearchthreads")
        .and_then(|s| s.parse::<u32>().ok())
        .is_some_and(|n| n > 0)
}
pub fn add_overrides(spec: &mut LaunchSpec, values: impl IntoIterator<Item = (String, String)>) {
    let mut incoming = values.into_iter().peekable();
    if incoming.peek().is_none() {
        return;
    }
    let mut merged = BTreeMap::<String, String>::new();
    for pair in spec.args.windows(2) {
        if matches!(pair[0].as_str(), "-override-config" | "--override-config") {
            for item in pair[1].split(',') {
                if let Some((key, value)) = item.split_once('=') {
                    let key = key.trim();
                    merged.retain(|old, _| !old.eq_ignore_ascii_case(key));
                    merged.insert(key.to_string(), value.trim().to_string());
                }
            }
        }
    }
    let mut remove_next = false;
    spec.args.retain(|arg| {
        if remove_next {
            remove_next = false;
            return false;
        }
        if matches!(arg.as_str(), "-override-config" | "--override-config") {
            remove_next = true;
            return false;
        }
        true
    });
    for (key, value) in incoming {
        merged.retain(|old, _| !old.eq_ignore_ascii_case(&key));
        merged.insert(key, value);
    }
    let mut config = String::new();
    for (key, value) in merged {
        if !config.is_empty() {
            config.push(',');
        }
        config.push_str(&key);
        config.push('=');
        config.push_str(&value);
    }
    spec.args.extend(["-override-config".into(), config]);
}

/// Java `EngineThreadPolicy.environment`: file sizes/mtime, command and execution directory.
pub fn environment(command: &str, spec: &LaunchSpec) -> Result<Value, String> {
    fn state(path: &Path) -> Result<Value, String> {
        let meta = path.metadata().map_err(|_| "EngineThreadPolicy.environmentUnknown")?;
        if !meta.is_file() {
            return Err("EngineThreadPolicy.environmentUnknown".into());
        }
        let path = std::fs::canonicalize(path).map_err(|_| "EngineThreadPolicy.environmentUnknown")?;
        let modified = meta
            .modified()
            .ok()
            .and_then(|s| s.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |s| s.as_millis() as u64);
        Ok(serde_json::json!({"path":path.to_string_lossy(), "size":meta.len(), "modified":modified}))
    }
    let directory = spec
        .working_dir
        .clone()
        .or_else(|| std::env::current_dir().ok())
        .ok_or("EngineThreadPolicy.environmentUnknown")?;
    let resolve = |s: &str| {
        let p = PathBuf::from(s);
        if p.is_absolute() {
            p
        } else {
            directory.join(p)
        }
    };
    let args = split_option_values(&spec.args);
    let mut configs = args
        .windows(2)
        .filter(|p| matches!(p[0].as_str(), "-config" | "--config"))
        .map(|p| resolve(&p[1]));
    let config = configs.next().ok_or("EngineThreadPolicy.unknownBenchmarkTarget")?;
    let model = args
        .windows(2)
        .find(|p| matches!(p[0].as_str(), "-model" | "--model" | "-weights" | "--weights"))
        .map(|p| resolve(&p[1]))
        .ok_or("EngineThreadPolicy.unknownBenchmarkTarget")?;
    let engine = if Path::new(&spec.program).is_file() {
        PathBuf::from(&spec.program)
    } else {
        std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default())
            .map(|p| p.join(&spec.program))
            .find(|p| p.is_file())
            .ok_or("EngineThreadPolicy.environmentUnknown")?
    };
    let mut value = serde_json::json!({"engine":state(&engine)?,"config":state(&config)?,"model":state(&model)?,"command":command,"workingDirectory":directory.to_string_lossy()});
    let extra: Result<Vec<_>, _> = configs.map(|p| state(&p)).collect();
    let extra = extra?;
    if !extra.is_empty() {
        value["additionalConfigs"] = Value::Array(extra);
    }
    Ok(value)
}

/// Java `applyEntryLaunchPolicy`: user GPU overrides win; search threads are profile-owned.
pub fn apply_launch_policy(command: &str, policy: Option<&ThreadPolicy>, spec: &mut LaunchSpec) {
    if !is_local_katago(command) {
        return;
    }
    let Some(policy) = policy else { return };
    let existing = overrides(&spec.args);
    let mut managed = BTreeMap::new();
    if !cfg!(all(target_os = "macos", target_arch = "aarch64"))
        && policy
            .environment
            .as_ref()
            .is_some_and(|old| environment(command, spec).as_ref() == Ok(old))
    {
        for (key, n) in [
            ("numNNServerThreadsPerModel", policy.nn_server_threads),
            ("nnMaxBatchSize", policy.batch_size),
        ] {
            if n > 0 && !existing.contains_key(&key.to_ascii_lowercase()) {
                managed.insert(key.to_string(), n.to_string());
            }
        }
    }
    let threads = policy.threads_for_launch(command);
    if threads > 0 {
        managed.insert("numSearchThreads".into(), threads.to_string());
    }
    add_overrides(spec, managed);
}

/// Apple hardware profiles never follow a result onto a different chip/OS or changed artifacts.
pub fn apply_apple_launch_policy(command: &str, policy: &ThreadPolicy, spec: &mut LaunchSpec, hardware: &Value) {
    let Some(stored) = &policy.tuning_profiles else { return };
    let official = stored["mode"] == "OFFICIAL_THREADS";
    let Some(expected) = stored["fingerprintDigest"].as_str() else {
        return;
    };
    let Ok(current) = apple_fingerprint(command, spec, hardware, official) else {
        return;
    };
    if expected != current {
        return;
    }
    if official {
        return;
    }
    let candidate = if let Ok(candidate) =
        serde_json::from_value::<super::benchmark::AppleCandidate>(stored["candidate"].clone())
    {
        candidate
    } else {
        let Ok(devices) = serde_json::from_value::<Vec<u32>>(stored["devices"].clone()) else {
            return;
        };
        let Some(batch) = stored["batch"].as_u64().filter(|n| *n > 0 && *n <= 65536) else {
            return;
        };
        if devices.is_empty()
            || devices.iter().any(|d| !matches!(d, 0 | 100))
            || batch == 1 && devices.contains(&0) && devices.contains(&100)
        {
            return;
        }
        super::benchmark::AppleCandidate {
            id: policy.topology.clone(),
            devices,
            batch: batch as u32,
        }
    };
    let existing = overrides(&gtp::split_command(command));
    let user_topology = existing.keys().any(|k| metal_topology_key(k));
    add_overrides(
        spec,
        candidate.overrides().into_iter().filter(|(key, _)| {
            !(existing.contains_key(&key.to_ascii_lowercase()) || user_topology && key != "nnMaxBatchSize")
        }),
    );
}

/// Java `KataGoMeasuredReport.parse/assess`: evidence, not report command text, authorizes overlays.
pub fn validate_measured_report(report: &Value) -> Result<(), String> {
    fn number(v: &Value, zero: bool) -> Result<f64, String> {
        v.as_f64()
            .filter(|n| n.is_finite() && if zero { *n >= 0.0 } else { *n > 0.0 })
            .ok_or_else(|| "Invalid measured numeric value".into())
    }
    fn integer(v: &Value, zero: bool) -> Result<u64, String> {
        let n = number(v, zero)?;
        if n.fract() != 0.0 || n > i32::MAX as f64 {
            return Err("Invalid measured integer".into());
        }
        Ok(n as u64)
    }
    fn hash(v: &Value) -> bool {
        v.as_str()
            .is_some_and(|s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b)))
    }
    fn median(mut v: Vec<f64>) -> f64 {
        v.sort_by(f64::total_cmp);
        v[v.len() / 2]
    }
    fn cv(v: &[f64]) -> f64 {
        let scale = v.iter().copied().fold(0.0, f64::max);
        let mean = v.iter().map(|n| n / scale).sum::<f64>() / v.len() as f64;
        (v.iter().map(|n| (n / scale - mean).powi(2)).sum::<f64>() / (v.len() - 1) as f64).sqrt() / mean
    }
    if report["schemaVersion"] != 1
        || report["measurementMode"] != "application"
        || report["metricScope"] != "totalGpu"
        || !hash(&report["fixtureSha256"])
    {
        return Err("Unsupported measured tuning report".into());
    }
    let live = match report["scene"].as_str() {
        Some("live") => true,
        Some("whole-game") => false,
        _ => return Err("Unsupported measured scene".into()),
    };
    let budget = integer(&report["budget"], false)?;
    let positions = integer(&report["positions"], false)?;
    if live && positions != 1 {
        return Err("Live reports require one position".into());
    }
    let fingerprint = &report["fingerprint"];
    for key in ["engineSha256", "configSha256", "modelSha256"] {
        if !hash(&fingerprint[key]) {
            return Err("Invalid artifact hash".into());
        }
    }
    for key in ["gpuName", "driverVersion"] {
        if fingerprint[key].as_str().is_none_or(|s| s.trim().is_empty()) {
            return Err("Incomplete GPU identity".into());
        }
    }
    let total_memory = number(&fingerprint["memoryMiB"], false)?;
    if !fingerprint["commandSemantics"].is_object()
        || fingerprint["configIncludes"]
            .as_array()
            .is_none_or(|a| a.iter().any(|v| !hash(v)))
    {
        return Err("Invalid measured fingerprint".into());
    }
    let keys: &[&str] = if live {
        &["numSearchThreads", "nnMaxBatchSize"]
    } else {
        &[
            "numAnalysisThreads",
            "numSearchThreadsPerAnalysisThread",
            "nnMaxBatchSize",
        ]
    };
    for name in ["baselineParameters", "candidateParameters"] {
        let object = report[name].as_object().ok_or("Missing measured parameters")?;
        if object.len() != keys.len() || object.keys().any(|key| !keys.contains(&key.as_str())) {
            return Err("Unexpected or missing tuning parameter".into());
        }
        for key in keys {
            let n = integer(&object[*key], false)?;
            if n > if *key == "nnMaxBatchSize" { 65536 } else { 4096 } {
                return Err("Tuning parameter exceeds supported range".into());
            }
        }
    }
    if report["baselineParameters"] == report["candidateParameters"] {
        return Err("Candidate parameters are identical to baseline".into());
    }
    let runs = report["runs"]
        .as_array()
        .filter(|a| matches!(a.len(), 6 | 10))
        .ok_or("Exactly three or five complete warm pairs are required")?;
    let mut base: Vec<&Value> = Vec::new();
    let mut next: Vec<&Value> = Vec::new();
    let mut speeds = Vec::new();
    let mut previous = "";
    for (index, pair) in runs.as_chunks::<2>().0.iter().enumerate() {
        let first = pair[0]["profile"].as_str().unwrap_or("");
        if !matches!(first, "baseline" | "candidate")
            || first == previous
            || pair[1]["profile"].as_str() != Some(if first == "baseline" { "candidate" } else { "baseline" })
        {
            return Err("Rounds must alternate complete pairs".into());
        }
        previous = first;
        for run in pair {
            if run["phase"] != "warm"
                || integer(&run["round"], false)? != index as u64 + 1
                || integer(&run["maxEngineProcesses"], true)? != 1
            {
                return Err("Invalid measured run ownership".into());
            }
            let elapsed = number(&run["seconds"], false)?;
            if live {
                if number(&run["firstResultSeconds"], false)? > elapsed
                    || run.get("rootVisitsByTurn").is_some()
                    || integer(&run["observedRootVisits"], true)? < budget
                {
                    return Err("Incomplete live measurement".into());
                }
            } else {
                if run.get("firstResultSeconds").is_some() || run.get("observedRootVisits").is_some() {
                    return Err("Whole-game run requires per-turn visits only".into());
                }
                let visits = run["rootVisitsByTurn"]
                    .as_array()
                    .filter(|a| a.len() == positions as usize)
                    .ok_or("Missing visit counts")?;
                for v in visits {
                    if integer(v, true)? < budget {
                        return Err("Visit budget not completed".into());
                    }
                }
            }
            number(&run["responseSeconds"], true)?;
            number(&run["edtP95Seconds"], true)?;
            if number(&run["maxMemoryMiB"], false)? > total_memory {
                return Err("Recorded GPU memory exceeds physical memory".into());
            }
        }
        let (b, n) = if first == "baseline" {
            (&pair[0], &pair[1])
        } else {
            (&pair[1], &pair[0])
        };
        let speed = number(&b["seconds"], false)? / number(&n["seconds"], false)?;
        if !speed.is_finite() || speed <= 1.0 {
            return Err("Candidate must improve every pair".into());
        }
        base.push(b);
        next.push(n);
        speeds.push(speed);
    }
    if median(speeds) < 1.03 {
        return Err("Median speedup is below 3%".into());
    }
    let limit = if base.len() == 3 { 0.10 } else { 0.15 };
    for group in [&base, &next] {
        if cv(&group
            .iter()
            .map(|r| r["seconds"].as_f64().unwrap_or(0.0))
            .collect::<Vec<_>>())
            > limit
        {
            return Err("Timing evidence is too noisy".into());
        }
    }
    for key in if live {
        vec!["firstResultSeconds", "responseSeconds", "edtP95Seconds"]
    } else {
        vec!["responseSeconds", "edtP95Seconds"]
    } {
        let b: Vec<_> = base.iter().map(|r| r[key].as_f64().unwrap_or(0.0)).collect();
        let n: Vec<_> = next.iter().map(|r| r[key].as_f64().unwrap_or(0.0)).collect();
        if median(n.clone()) > median(b.clone()) * 1.10 + 0.005
            || n.iter().copied().fold(0.0, f64::max) > b.iter().copied().fold(0.0, f64::max) * 1.25 + 0.020
        {
            return Err("Measured latency regressed".into());
        }
    }
    let max_memory = |group: &Vec<&Value>| {
        group
            .iter()
            .map(|r| r["maxMemoryMiB"].as_f64().unwrap_or(0.0))
            .fold(0.0, f64::max)
    };
    if max_memory(&next) > max_memory(&base) * 1.15 + 256.0 || max_memory(&next) > total_memory * 0.90 {
        return Err("Measured GPU memory regression or insufficient headroom".into());
    }
    Ok(())
}

/// Content identity is freshly read; preserved timestamps cannot hide an artifact replacement.
pub fn sha256(path: &Path) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    use std::io::Read;
    let before = path.metadata().map_err(|e| e.to_string())?;
    if !before.is_file() {
        return Err("Fingerprint input is not a regular file".into());
    }
    let mut file = std::fs::File::open(path).map_err(|e| e.to_string())?;
    let mut digest = Sha256::new();
    let mut buffer = [0u8; 65536];
    loop {
        let count = file.read(&mut buffer).map_err(|e| e.to_string())?;
        if count == 0 {
            break;
        }
        digest.update(&buffer[..count]);
    }
    let after = path.metadata().map_err(|e| e.to_string())?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        if before.ino() != after.ino() || before.dev() != after.dev() {
            return Err("Fingerprint input changed while reading".into());
        }
    }
    if before.len() != after.len() || before.modified().ok() != after.modified().ok() {
        return Err("Fingerprint input changed while reading".into());
    }
    Ok(format!("{:x}", digest.finalize()))
}

/// Java `KataGoMeasuredFingerprint.capture`; hardware is supplied by the shell probe.
pub fn measured_fingerprint(
    target: &super::benchmark::Target,
    gpu: &str,
    driver: &str,
    memory_mib: u64,
) -> Result<Value, String> {
    fn includes(
        path: &Path,
        visited: &mut std::collections::BTreeSet<PathBuf>,
        out: &mut Vec<String>,
        include_self: bool,
    ) -> Result<(), String> {
        let real = path.canonicalize().map_err(|e| e.to_string())?;
        if visited.len() >= 64 || !visited.insert(real.clone()) {
            return Err("Repeated or excessive config includes".into());
        }
        if real.metadata().map_err(|e| e.to_string())?.len() > 4 * 1024 * 1024 {
            return Err("Configuration file too large".into());
        }
        if include_self {
            out.push(sha256(&real)?);
        }
        for raw in std::fs::read_to_string(&real).map_err(|e| e.to_string())?.lines() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if !line.starts_with('@') {
                continue;
            }
            let value = line
                .strip_prefix("@include")
                .filter(|s| s.starts_with(|c: char| c.is_whitespace() || c == '='))
                .ok_or("Unsupported config directive")?
                .trim_start_matches(|c: char| c.is_whitespace() || c == '=')
                .trim()
                .trim_matches(['\'', '"']);
            if value.is_empty() || Path::new(value).is_absolute() {
                return Err("Unsupported config include path".into());
            }
            includes(
                &real.parent().ok_or("Missing config directory")?.join(value),
                visited,
                out,
                true,
            )?;
        }
        Ok(())
    }
    if gpu.trim().is_empty() || driver.trim().is_empty() || memory_mib == 0 {
        return Err("Incomplete local engine or GPU identity".into());
    }
    let args = split_option_values(&target.launch.args);
    let mut i = 1;
    if !matches!(args.first().map(String::as_str), Some("gtp" | "analysis")) {
        return Err("Only direct local GTP or analysis commands support measured profiles".into());
    }
    while i < args.len() {
        match args[i].as_str() {
            "-model" | "--model" | "-weights" | "--weights" | "-config" | "--config" | "-override-config"
            | "--override-config" => {
                if i + 1 >= args.len() {
                    return Err("Missing command argument".into());
                }
                i += 2;
            }
            "-quit-without-waiting" => i += 1,
            _ => return Err("Unmeasured command argument".into()),
        }
    }
    let mut included = Vec::new();
    let mut visited = std::collections::BTreeSet::new();
    let config = Path::new(target.environment["config"]["path"].as_str().ok_or("Missing config")?);
    includes(config, &mut visited, &mut included, false)?;
    if let Some(additional) = target.environment["additionalConfigs"].as_array() {
        for file in additional {
            includes(
                Path::new(file["path"].as_str().ok_or("Missing config")?),
                &mut visited,
                &mut included,
                true,
            )?;
        }
    }
    // Capture the launch semantics before the measured overlay, including managed GPU settings.
    let mut semantics = BTreeMap::new();
    for pair in args.windows(2) {
        if matches!(pair[0].as_str(), "-override-config" | "--override-config") {
            for item in pair[1].split(',') {
                if let Some((key, value)) = item.split_once('=') {
                    if ![
                        "numSearchThreads",
                        "numAnalysisThreads",
                        "numSearchThreadsPerAnalysisThread",
                        "nnMaxBatchSize",
                        "homeDataDir",
                        "logDir",
                        "logFile",
                        "logDirDated",
                    ]
                    .contains(&key.trim())
                    {
                        semantics.insert(key.trim().to_string(), value.trim().to_string());
                    }
                }
            }
        }
    }
    Ok(
        serde_json::json!({"engineSha256":sha256(Path::new(target.environment["engine"]["path"].as_str().ok_or("Missing engine")?))?,"modelSha256":sha256(Path::new(target.environment["model"]["path"].as_str().ok_or("Missing model")?))?,"configSha256":sha256(config)?,"configIncludes":included,"gpuName":gpu,"driverVersion":driver,"memoryMiB":memory_mib,"commandSemantics":semantics}),
    )
}

/// Java `MeasuredKataGoTuning.applyOverlay`: fresh content identity and reviewed numeric keys only.
pub fn apply_measured_live(
    command: &str,
    policy: &ThreadPolicy,
    roots: &[PathBuf],
    spec: &mut LaunchSpec,
    gpu: &str,
    driver: &str,
    memory: u64,
) {
    let Some(stored) = policy.measured_scenes.as_ref().and_then(|s| s.get("live")) else {
        return;
    };
    if stored["schemaVersion"] != 1
        || stored["command"].as_str() != Some(command)
        || stored["report"]["scene"] != "live"
        || validate_measured_report(&stored["report"]).is_err()
    {
        return;
    }
    let Ok(mut target) = super::benchmark::Target::capture(command, roots) else {
        return;
    };
    target.launch = spec.clone();
    if measured_fingerprint(&target, gpu, driver, memory).as_ref() != Ok(&stored["report"]["fingerprint"]) {
        return;
    }
    if let Some(parameters) = stored["report"]["candidateParameters"].as_object() {
        add_overrides(
            spec,
            parameters
                .iter()
                .filter_map(|(k, v)| v.as_u64().map(|n| (k.clone(), n.to_string()))),
        );
    }
}

fn metal_topology_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    key == "numnnserverthreadspermodel"
        || [
            "metaldevicetouse",
            "metalgputouse",
            "devicetouse",
            "gputouse",
            "metalusefp16",
            "usefp16",
        ]
        .iter()
        .any(|prefix| key.starts_with(prefix))
}

/// Whether a stored Apple profile still matches this hardware and the freshly read artifacts.
/// Reads files: call it off the workbench.
pub fn apple_profile_matches(profile: &super::profile::EngineProfile, roots: &[PathBuf], hardware: &Value) -> bool {
    let Some(stored) = profile.thread_policy.as_ref().and_then(|p| p.tuning_profiles.as_ref()) else {
        return false;
    };
    let Some(expected) = stored["fingerprintDigest"].as_str() else {
        return false;
    };
    let Some(spec) = gtp::prepare_launch(&profile.command, roots) else {
        return false;
    };
    apple_fingerprint(&profile.command, &spec, hardware, stored["mode"] == "OFFICIAL_THREADS")
        .is_ok_and(|current| current == expected)
}

/// Java `KataGoTuningFingerprint.computeCanonicalDigest` (schema 1, planner 3).
/// Length-prefixed UTF-8 fields preserve imported Java profiles without trusting their paths.
pub fn apple_fingerprint(command: &str, spec: &LaunchSpec, hardware: &Value, official: bool) -> Result<String, String> {
    use sha2::{Digest, Sha256};
    fn field(hash: &mut Sha256, key: &str, value: &str) {
        hash.update((key.len() as u32).to_be_bytes());
        hash.update(key.as_bytes());
        hash.update((value.len() as u32).to_be_bytes());
        hash.update(value.as_bytes());
    }
    let environment = environment(command, spec)?;
    let mut hash = Sha256::new();
    field(&mut hash, "schemaVersion", "1");
    field(&mut hash, "plannerVersion", "3");
    for (name, key) in [
        ("hardwareModel", "model"),
        ("chipOrBrand", "chipName"),
        ("architecture", "architecture"),
        ("logicalCpuCount", "logicalCores"),
        ("memoryBytes", "memoryBytes"),
        ("macOsBuild", "osBuild"),
    ] {
        field(
            &mut hash,
            &format!("host.{name}"),
            hardware[key].as_str().ok_or("Incomplete Apple hardware identity")?,
        );
    }
    field(
        &mut hash,
        "host.rosettaTranslated",
        if hardware["rosetta"].as_str() == Some("1") {
            "true"
        } else {
            "false"
        },
    );
    for key in ["engine", "model", "config"] {
        let path = Path::new(
            environment[key]["path"]
                .as_str()
                .ok_or("Missing fingerprint artifact")?,
        );
        field(&mut hash, &format!("{key}.sha256"), &sha256(path)?);
        field(
            &mut hash,
            &format!("{key}.sizeBytes"),
            &environment[key]["size"].as_u64().unwrap_or(0).to_string(),
        );
    }
    let mut canonical = BTreeMap::new();
    for pair in split_option_values(&spec.args).windows(2) {
        if matches!(pair[0].as_str(), "-override-config" | "--override-config") {
            for item in pair[1].split(',') {
                if let Some((key, value)) = item.split_once('=') {
                    let key = key.trim();
                    let lower = key.to_ascii_lowercase();
                    let common = [
                        "numsearchthreads",
                        "numanalysisthreads",
                        "numsearchthreadsperanalysisthread",
                        "homedatadir",
                        "analysispvlen",
                        "logtostderr",
                        "logallgtpcommunication",
                        "logsearchinfo",
                    ]
                    .contains(&lower.as_str());
                    if !common && (official || !metal_topology_key(key) && lower != "nnmaxbatchsize") {
                        canonical.insert(if official { lower } else { key.to_string() }, value.trim().to_string());
                    }
                }
            }
        }
    }
    let mut semantics = String::new();
    for (key, value) in canonical {
        semantics.push_str(&format!("{key}={value}\n"));
    }
    if let Some(configs) = environment["additionalConfigs"].as_array() {
        for config in configs {
            semantics.push_str(&format!(
                "configLayer={}|{}|{}\n",
                config["path"].as_str().unwrap_or(""),
                config["size"].as_u64().unwrap_or(0),
                config["modified"].as_u64().unwrap_or(0)
            ));
        }
    }
    if official {
        semantics.push_str("benchmarkMode=officialThreads\nbenchmarkTimeSeconds=5\n");
    }
    field(&mut hash, "commandSemantics", semantics.trim());
    Ok(format!("{:x}", hash.finalize()))
}
