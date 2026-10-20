//! KataGo 1.17/1.18 benchmark protocol (`KataGoBenchmarkParser`, `KataGoRuntimeHelper`).
use super::gtp::{prepare_launch, LaunchSpec};
use super::tuning;
use serde::{Deserialize, Serialize};
use std::path::PathBuf;

#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Observation {
    pub backend: String,
    pub current_threads: u32,
    pub recommended_threads: u32,
    pub nn_server_threads: u32,
    pub batch_size: u32,
    pub visits_per_second: f64,
    pub nn_evals_per_second: f64,
    pub average_batch_size: f64,
    pub failure: bool,
    pub metrics: Vec<Metrics>,
    pub apple_candidate: Option<AppleCandidate>,
    pub mps_graph_initialized: bool,
    pub core_ml_initialized: bool,
    pub apple_hardware: Option<serde_json::Value>,
    pub apple_fingerprint: Option<String>,
}
#[derive(Clone, Debug, Default, PartialEq, Serialize, Deserialize)]
pub struct Metrics {
    pub threads: u32,
    pub completed: u32,
    pub total: u32,
    pub visits: f64,
    pub nn_evals: f64,
    pub nn_batches: f64,
    pub average_batch: f64,
}
fn integer_after(line: &str, key: &str) -> Option<u32> {
    line.split_once(key)?
        .1
        .trim_start()
        .trim_start_matches('=')
        .trim_start()
        .split(|c: char| !c.is_ascii_digit())
        .next()?
        .parse()
        .ok()
}
fn number_after(line: &str, key: &str) -> Option<f64> {
    line.split_once(key)?
        .1
        .trim_start()
        .trim_start_matches('=')
        .split_whitespace()
        .next()?
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && *v >= 0.0)
}
fn failure_line(line: &str) -> bool {
    line.starts_with("error:")
        || line.starts_with("fatal:")
        || [
            "fatal error",
            "segmentation fault",
            "sigsegv",
            "core dumped",
            "uncaught exception",
            "terminate called",
            "failed to load model",
            "invalid metaldevicetousethread",
        ]
        .iter()
        .any(|s| line.contains(s))
}
pub fn parse(output: &str, explicit_threads: u32) -> Observation {
    let mut out = Observation::default();
    let mut additional = false;
    let mut tuning_gpu = false;
    let mut recoverable = false;
    let mut server_block = false;
    for line in output.split(['\r', '\n']).map(str::trim).filter(|s| !s.is_empty()) {
        let lower = line.to_ascii_lowercase();
        if let Some(backend) = line
            .strip_prefix("You are currently using the ")
            .and_then(|s| s.strip_suffix(" version of KataGo."))
        {
            out.backend = backend.to_string();
        }
        if line.contains("Your GTP config is currently set to use numSearchThreads") {
            out.current_threads = integer_after(line, "numSearchThreads").unwrap_or(0);
        }
        if line.starts_with("numSearchThreads") && line.ends_with("(recommended)") {
            out.recommended_threads = integer_after(line, "numSearchThreads").unwrap_or(0);
        }
        if line.starts_with("Running additional tests of a few other settings") {
            additional = true;
        }
        if line.starts_with("ADDITIONAL RECOMMENDATION:") {
            if let Some(n) = integer_after(&lower, "set numnnserverthreadspermodel") {
                out.nn_server_threads = n;
                server_block = false;
            }
            if let Some(n) = integer_after(&lower, "set nnmaxbatchsize") {
                out.batch_size = n;
            }
            if line.starts_with("ADDITIONAL RECOMMENDATION: 2 NN server threads per GPU measured faster.") {
                server_block = true;
            }
        }
        if server_block && lower.starts_with("numnnserverthreadspermodel") {
            out.nn_server_threads = integer_after(&lower, "numnnserverthreadspermodel").unwrap_or(0);
            server_block = false;
        }
        if !additional && line.starts_with("numSearchThreads") {
            if let Some((threads, done, total)) = positions(line) {
                if let (Some(visits), Some(nn_evals), Some(nn_batches), Some(average_batch)) = (
                    number_after(line, "visits/s"),
                    number_after(line, "nnEvals/s"),
                    number_after(line, "nnBatches/s"),
                    number_after(line, "avgBatchSize"),
                ) {
                    if threads > 0
                        && total > 0
                        && done <= total
                        && visits >= 0.0
                        && nn_evals >= 0.0
                        && nn_batches >= 0.0
                        && average_batch >= 0.0
                    {
                        let metric = Metrics {
                            threads,
                            completed: done,
                            total,
                            visits,
                            nn_evals,
                            nn_batches,
                            average_batch,
                        };
                        if let Some(old) = out.metrics.iter_mut().find(|m| m.threads == threads) {
                            *old = metric;
                        } else {
                            out.metrics.push(metric);
                        }
                    }
                }
            }
        }
        if lower.contains("gpu mode - using mpsgraph") || lower.contains("initialized mpsgraph gpu-only mode") {
            out.mps_graph_initialized = true;
        }
        if lower.contains("mux ane mode - using coreml") {
            out.core_ml_initialized = true;
        }
        if line.starts_with("Beginning GPU tuning") {
            if tuning_gpu && recoverable {
                out.failure = true;
            }
            tuning_gpu = true;
            recoverable = false;
        }
        if failure_line(&lower) {
            if tuning_gpu && lower.starts_with("error:") && !failure_line(lower.trim_start_matches("error:").trim()) {
                recoverable = true;
            } else {
                out.failure = true;
            }
        }
        if tuning_gpu && line == "Done tuning" {
            tuning_gpu = false;
            recoverable = false;
        }
    }
    out.failure |= recoverable;
    if out.recommended_threads == 0 {
        out.recommended_threads = explicit_threads;
    }
    if let Some(m) = out.metrics.iter().find(|m| m.threads == out.recommended_threads) {
        out.visits_per_second = m.visits;
        out.nn_evals_per_second = m.nn_evals;
        out.average_batch_size = m.average_batch;
    }
    out
}
/// Compatibility probe uses a fixed one-thread observation, not a recommendation marker.
pub fn probe_passed(output: &str) -> bool {
    let result = parse(output, 1);
    !result.failure
        && result
            .metrics
            .iter()
            .any(|m| m.threads == 1 && m.completed == m.total && m.visits > 0.0)
}
fn positions(line: &str) -> Option<(u32, u32, u32)> {
    let (lhs, rhs) = line.strip_prefix("numSearchThreads")?.split_once(':')?;
    let threads = lhs.trim().trim_start_matches('=').trim().parse().ok()?;
    let (done, rest) = rhs.trim().split_once('/')?;
    let total = rest.split_whitespace().next()?.parse().ok()?;
    if !rest.contains("positions") {
        return None;
    }
    Some((threads, done.trim().parse().ok()?, total))
}
/// Java `BenchmarkProgressTracker`: actual completed work, including KataGo 1.18 extra tests.
#[derive(Clone, Debug, Default)]
pub struct Progress {
    expected: usize,
    completed: std::collections::BTreeMap<u32, u32>,
    phase: u8,
    pub permille: u16,
}
impl Progress {
    pub fn line(&mut self, line: &str) -> u16 {
        let line = line.trim();
        let mut next = self.permille;
        if line.contains("Loading model") || line.contains("Initializing benchmark") {
            next = 30;
        }
        if let Some(list) = line.strip_prefix("Possible numbers of threads to test:") {
            self.expected = self
                .expected
                .max(list.split_whitespace().filter(|s| s.parse::<u32>().is_ok()).count());
            next = 80;
        }
        for (text, phase, progress) in [
            ("Running additional tests of a few other settings", 1, 810),
            ("Re-measuring the current recommendation as a baseline", 1, 820),
            ("Testing 2 NN server threads per GPU", 2, 875),
            ("Testing a max batch size", 3, 930),
        ] {
            if line.contains(text) {
                self.phase = phase;
                next = progress;
            }
        }
        if line.contains("2 NN server threads per GPU was") {
            next = 920;
        }
        if line.contains("Half batch size was") {
            self.phase = 4;
            next = 980;
        }
        if line.starts_with("ADDITIONAL RECOMMENDATION:") {
            self.phase = 4;
            next = 990;
        }
        if line.contains("Ordered summary of results:") {
            next = if self.phase == 0 { 795 } else { 990 };
        }
        if let Some((thread, done, total)) = positions(line) {
            if total > 0 {
                if self.phase == 0 {
                    let count = self.completed.entry(thread).or_default();
                    *count = (*count).max(done.min(total));
                    if self.expected == 0 {
                        self.expected = 12;
                    }
                    self.expected = self.expected.max(self.completed.len());
                    let completed: u32 = self.completed.values().sum();
                    next = (100 + (completed as u64 * 680 / (self.expected as u64 * total as u64)).min(680)) as u16;
                } else {
                    let (start, span) = match self.phase {
                        2 => (875, 45),
                        3 => (930, 50),
                        _ => (820, 45),
                    };
                    next = (start + done.min(total) * span / total) as u16;
                }
            }
        }
        self.permille = self.permille.max(next.min(995));
        self.permille
    }
}
#[derive(Clone, Debug)]
pub struct Target {
    pub launch: LaunchSpec,
    pub environment: serde_json::Value,
    pub command: String,
}
#[derive(Clone)]
pub struct BenchmarkJob {
    pub id: u64,
    pub target: Target,
    pub cancel: std::sync::Arc<std::sync::atomic::AtomicBool>,
    pub experimental: bool,
}
impl Target {
    /// Java captureBenchmarkTarget: only a saved local KataGo with explicit config and model.
    pub fn capture(command: &str, roots: &[PathBuf]) -> Result<Self, String> {
        if !tuning::is_local_katago(command) {
            return Err("EngineThreadPolicy.unknownBenchmarkTarget".into());
        }
        let launch = prepare_launch(command, roots).ok_or("EngineThreadPolicy.unknownBenchmarkTarget")?;
        if launch.args.first().map(String::as_str) != Some("gtp") {
            return Err("EngineThreadPolicy.invalidBenchmarkCommand".into());
        }
        let mut index = 1;
        let mut models = 0;
        while index < launch.args.len() {
            let option = launch.args[index].as_str();
            if matches!(option, "-quit-without-waiting" | "--quit-without-waiting") {
                index += 1;
                continue;
            }
            if index + 1 >= launch.args.len() || launch.args[index + 1].trim().is_empty() {
                return Err("EngineThreadPolicy.invalidBenchmarkCommand".into());
            }
            match option {
                "-model" | "--model" | "-weights" | "--weights" => models += 1,
                "-config" | "--config" | "-override-config" | "--override-config" | "-human-model"
                | "--human-model" | "-override-version" | "--override-version" => {}
                _ => return Err("EngineThreadPolicy.invalidBenchmarkCommand".into()),
            }
            index += 2;
        }
        if models != 1 {
            return Err("EngineThreadPolicy.invalidBenchmarkCommand".into());
        }
        let environment = tuning::environment(command, &launch)?;
        Ok(Self {
            launch,
            environment,
            command: command.to_string(),
        })
    }
    /// Java Config defaults maxGameThinkingTimeSeconds to 2; resolveBenchmarkTimeSeconds clamps it to 5.
    pub fn benchmark(&self, seconds: u32) -> LaunchSpec {
        let mut spec = LaunchSpec {
            program: self.launch.program.clone(),
            working_dir: self.launch.working_dir.clone(),
            library_dirs: self.launch.library_dirs.clone(),
            args: vec!["benchmark".into()],
        };
        let mut inherited = std::collections::BTreeMap::new();
        let mut index = 1;
        while index < self.launch.args.len() {
            if matches!(
                self.launch.args[index].as_str(),
                "-quit-without-waiting" | "--quit-without-waiting"
            ) {
                index += 1;
                continue;
            }
            let pair = &self.launch.args[index..index + 2];
            match pair[0].as_str() {
                "-model" | "--model" | "-weights" | "--weights" => spec.args.extend(["-model".into(), pair[1].clone()]),
                "-config" | "--config" => spec.args.extend(["-config".into(), pair[1].clone()]),
                "-override-version" | "--override-version" => spec.args.extend([pair[0].clone(), pair[1].clone()]),
                "-override-config" | "--override-config" => {
                    for item in pair[1].split(',') {
                        if let Some((key, value)) = item.split_once('=') {
                            if ![
                                "numSearchThreads",
                                "numAnalysisThreads",
                                "numSearchThreadsPerAnalysisThread",
                            ]
                            .iter()
                            .any(|k| k.eq_ignore_ascii_case(key.trim()))
                            {
                                inherited.insert(key.trim().to_string(), value.trim().to_string());
                            }
                        }
                    }
                }
                _ => {}
            }
            index += 2;
        }
        spec.args.extend([
            "-s".into(),
            "-n".into(),
            "6".into(),
            "-v".into(),
            "800".into(),
            "-time".into(),
            seconds.clamp(5, 15).to_string(),
        ]);
        for key in ["logToStderr", "logAllGTPCommunication", "logSearchInfo"] {
            inherited.insert(key.into(), "false".into());
        }
        tuning::add_overrides(&mut spec, inherited);
        spec
    }
}

/// Java `AppleSiliconTuningPlanner`: bounded memory-aware topology candidates.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct AppleCandidate {
    pub id: String,
    pub devices: Vec<u32>,
    pub batch: u32,
}
impl AppleCandidate {
    pub fn overrides(&self) -> Vec<(String, String)> {
        let mut out = vec![
            ("numNNServerThreadsPerModel".into(), self.devices.len().to_string()),
            ("metalUseFP16-0".into(), "true".into()),
            ("nnMaxBatchSize".into(), self.batch.to_string()),
        ];
        for (i, d) in self.devices.iter().enumerate() {
            out.push((format!("metalDeviceToUseModel0Thread{i}"), d.to_string()));
        }
        out
    }
    pub fn baseline(&self) -> bool {
        self.devices == [0] && self.batch == 1
    }
}
pub fn apple_candidates(memory: u64, model: u64) -> Vec<AppleCandidate> {
    let mut out = Vec::new();
    let mut add = |id: &str, devices: &[u32], batches: &[u32]| {
        for batch in batches {
            out.push(AppleCandidate {
                id: format!("{id}-b{batch}"),
                devices: devices.to_vec(),
                batch: *batch,
            });
        }
    };
    add("G", &[0], &[1, 2, 4]);
    add("GG", &[0, 0], &[2, 4]);
    add("GA", &[0, 100], &[2, 4]);
    if memory >= 12 * 1024 * 1024 * 1024 && (model == 0 || model <= 3 * 1024 * 1024 * 1024 / 2) {
        add("GGA", &[0, 0, 100], &[3, 6]);
    }
    if memory >= 16 * 1024 * 1024 * 1024 && (model == 0 || model <= 3 * 1024 * 1024 * 1024 / 2) {
        add("GGAA", &[0, 0, 100, 100], &[4, 8]);
    }
    out
}
pub fn apple_benchmark_args(args: &[String], candidate: &AppleCandidate, smoke: bool) -> Vec<String> {
    let mut args = args.to_vec();
    if smoke {
        args.retain(|s| s != "-s");
        args.extend(["-t".into(), "6".into()]);
    }
    for i in 0..args.len().saturating_sub(1) {
        if args[i] == "-n" {
            args[i + 1] = if smoke { "1" } else { "3" }.into();
        }
        if args[i] == "-v" {
            args[i + 1] = if smoke { "200" } else { "600" }.into();
        }
    }
    args.extend([
        "-boardsize".into(),
        "19".into(),
        "-fixed-batch-size".into(),
        candidate.batch.to_string(),
    ]);
    let mut spec = LaunchSpec {
        program: String::new(),
        args,
        working_dir: None,
        library_dirs: Vec::new(),
    };
    tuning::add_overrides(&mut spec, candidate.overrides());
    spec.args
}
pub fn apple_shortlist(mut results: Vec<(AppleCandidate, Observation)>) -> Result<Vec<AppleCandidate>, String> {
    let index = results
        .iter()
        .position(|(c, _)| c.baseline())
        .ok_or("The mandatory single-GPU baseline did not complete the experimental smoke test")?;
    let baseline = results.remove(index).0;
    results.sort_by(|a, b| {
        b.1.visits_per_second
            .total_cmp(&a.1.visits_per_second)
            .then(a.0.devices.len().cmp(&b.0.devices.len()))
            .then(a.0.batch.cmp(&b.0.batch))
    });
    let mut out = vec![baseline];
    out.extend(results.into_iter().take(2).map(|(c, _)| c));
    Ok(out)
}
/// Java selector: three fixed-thread samples, <=15% spread, >=15% gain over stable baseline.
pub fn apple_select(
    mut results: Vec<(AppleCandidate, Observation)>,
    samples: &std::collections::BTreeMap<String, Vec<Observation>>,
) -> Result<Observation, String> {
    fn aggregate(
        candidate: &AppleCandidate,
        official: &Observation,
        samples: &std::collections::BTreeMap<String, Vec<Observation>>,
    ) -> Option<Observation> {
        let mut values = samples.get(&candidate.id)?.clone();
        if values.len() != 3
            || values.iter().any(|o| {
                o.failure
                    || o.recommended_threads != official.recommended_threads
                    || o.visits_per_second <= 0.0
                    || !o
                        .metrics
                        .iter()
                        .any(|m| m.threads == o.recommended_threads && m.completed == m.total)
            })
        {
            return None;
        }
        values.sort_by(|a, b| a.visits_per_second.total_cmp(&b.visits_per_second));
        if (values[2].visits_per_second - values[0].visits_per_second) / values[1].visits_per_second > 0.15 {
            return None;
        }
        Some(values.remove(1))
    }
    let index = results
        .iter()
        .position(|(c, _)| c.baseline())
        .ok_or("The mandatory single-GPU baseline did not complete the official search")?;
    let (baseline, official) = results.remove(index);
    let representative = aggregate(&baseline, &official, samples)
        .ok_or("The single-GPU baseline stability check was inconclusive; keeping previous profile")?;
    let threshold = representative.visits_per_second * 1.15;
    let mut winner = (baseline, official, representative);
    for (candidate, official) in results {
        if candidate.devices.contains(&100) && official.recommended_threads < 2 {
            continue;
        }
        if let Some(measured) = aggregate(&candidate, &official, samples) {
            if measured.visits_per_second >= threshold && measured.visits_per_second > winner.2.visits_per_second {
                winner = (candidate, official, measured);
            }
        }
    }
    let mut observation = winner.1;
    observation.visits_per_second = winner.2.visits_per_second;
    observation.nn_evals_per_second = winner.2.nn_evals_per_second;
    observation.average_batch_size = winner.2.average_batch_size;
    observation.nn_server_threads = winner.0.devices.len() as u32;
    observation.batch_size = winner.0.batch;
    observation.apple_candidate = Some(winner.0);
    Ok(observation)
}

/// Java `buildBenchmarkSignature`: same artifact tuple as imported dismissal settings.
pub fn startup_signature(target: &Target) -> String {
    fn append(out: &mut String, path: Option<&std::path::Path>) {
        let Some(path) = path else {
            out.push_str("|missing");
            return;
        };
        out.push('|');
        out.push_str(&path.to_string_lossy());
        let state = path.metadata().ok();
        let size = state.as_ref().map_or(0, |m| m.len());
        let modified = state
            .and_then(|m| m.modified().ok())
            .and_then(|t| t.duration_since(std::time::UNIX_EPOCH).ok())
            .map_or(0, |d| d.as_millis() as u64);
        out.push_str(&format!(":{size}:{modified}"));
    }
    let e = &target.environment;
    let mut out = String::new();
    let path = |key: &str| e[key]["path"].as_str().map(std::path::Path::new);
    append(&mut out, path("engine"));
    append(&mut out, path("config"));
    let analysis = path("config")
        .and_then(std::path::Path::parent)
        .map(|p| p.join("analysis_example.cfg"));
    append(&mut out, analysis.as_deref());
    append(&mut out, path("model"));
    out
}

pub fn apple_cell_valid(candidate: &AppleCandidate, observation: &Observation) -> bool {
    !observation.failure
        && observation.recommended_threads > 0
        && observation.metrics.iter().any(|m| {
            m.threads == observation.recommended_threads && m.completed == m.total && m.total > 0 && m.visits > 0.0
        })
        && (!candidate.devices.contains(&0) || observation.mps_graph_initialized)
        && (!candidate.devices.contains(&100) || observation.core_ml_initialized)
}

/// Java `Collections.shuffle(Random)`: independently seeded balanced candidate rounds.
pub fn shuffle<T>(items: &mut [T], seed: &mut u64) {
    for i in (1..items.len()).rev() {
        let bound = (i + 1) as u64;
        let index = loop {
            *seed = seed.wrapping_mul(0x5DEECE66D).wrapping_add(0xB) & ((1u64 << 48) - 1);
            let bits = *seed >> 17;
            let value = bits % bound;
            if bits - value + bound - 1 < (1u64 << 31) {
                break value as usize;
            }
        };
        items.swap(i, index);
    }
}
