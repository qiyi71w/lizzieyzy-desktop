//! Native tuning processes are isolated, cancellation-bound and never run under the workbench lock.
use crate::State;
use lizzie_core::api::{ApiError, CommandResult, ErrorKind, TuningCommand, TuningResult};
use lizzie_core::engine::{
    benchmark::{self, BenchmarkJob, Observation},
    gtp::LaunchSpec,
    gtp_config, tuning,
};
use std::collections::BTreeMap;
use std::io::{Read, Write};
use std::process::{Child, ChildStdin, Command, Stdio};
use std::sync::{
    atomic::{AtomicBool, AtomicU64, Ordering},
    mpsc, Arc, LazyLock, Mutex,
};
use std::time::{Duration, Instant};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

static NEXT: AtomicU64 = AtomicU64::new(1);
static ENDING: AtomicBool = AtomicBool::new(false);
static CHILDREN: LazyLock<Mutex<BTreeMap<u64, Arc<Mutex<Child>>>>> = LazyLock::new(Mutex::default);
fn registry() -> &'static Mutex<BTreeMap<u64, Arc<Mutex<Child>>>> {
    &CHILDREN
}
struct Process {
    id: u64,
    child: Arc<Mutex<Child>>,
}
impl Drop for Process {
    fn drop(&mut self) {
        let mut c = self.child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
        drop(c);
        registry().lock().unwrap().remove(&self.id);
    }
}
fn spawn(spec: &LaunchSpec) -> Result<Process, String> {
    if ENDING.load(Ordering::Acquire) {
        return Err("Application is exiting".into());
    }
    let mut cmd = Command::new(&spec.program);
    cmd.args(&spec.args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(dir) = &spec.working_dir {
        cmd.current_dir(dir);
    }
    #[cfg(windows)]
    {
        use std::os::windows::process::CommandExt;
        cmd.creation_flags(0x08000000);
        if !spec.library_dirs.is_empty() {
            let mut paths = spec.library_dirs.clone();
            paths.extend(std::env::split_paths(&std::env::var_os("PATH").unwrap_or_default()));
            cmd.env("PATH", std::env::join_paths(paths).map_err(|e| e.to_string())?);
        }
    }
    let child = Arc::new(Mutex::new(cmd.spawn().map_err(|e| e.to_string())?));
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let mut children = registry().lock().unwrap();
    if ENDING.load(Ordering::Acquire) {
        let mut c = child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
        return Err("Application is exiting".into());
    }
    children.insert(id, child.clone());
    Ok(Process { id, child })
}
/// Called on every application exit path; returns only after helper processes are reaped.
pub fn shutdown() {
    ENDING.store(true, Ordering::Release);
    let children: Vec<_> = registry().lock().unwrap().values().cloned().collect();
    for child in children {
        let mut c = child.lock().unwrap();
        let _ = c.kill();
        let _ = c.wait();
    }
}
fn lines(mut reader: impl Read, tx: mpsc::Sender<String>) {
    let mut buffer = [0; 8192];
    let mut pending = Vec::new();
    loop {
        let n = match reader.read(&mut buffer) {
            Ok(0) | Err(_) => break,
            Ok(n) => n,
        };
        for byte in &buffer[..n] {
            if matches!(*byte, b'\r' | b'\n') {
                if !pending.is_empty() {
                    let _ = tx.send(String::from_utf8_lossy(&pending).into_owned());
                    pending.clear();
                }
            } else if pending.len() < 65536 {
                pending.push(*byte);
            }
        }
    }
    if !pending.is_empty() {
        let _ = tx.send(String::from_utf8_lossy(&pending).into_owned());
    }
}
fn collect(
    spec: &LaunchSpec,
    cancel: &AtomicBool,
    timeout: Duration,
    mut progress: impl FnMut(&str),
) -> Result<String, String> {
    if cancel.load(Ordering::Acquire) || ENDING.load(Ordering::Acquire) {
        return Err("AutoSetup.benchmarkCancelled".into());
    }
    let process = spawn(spec)?;
    let (stdout, stderr) = {
        let mut c = process.child.lock().unwrap();
        (
            c.stdout.take().ok_or("Missing stdout")?,
            c.stderr.take().ok_or("Missing stderr")?,
        )
    };
    let (tx, rx) = mpsc::channel();
    let tx2 = tx.clone();
    let a = std::thread::spawn(move || lines(stdout, tx));
    let b = std::thread::spawn(move || lines(stderr, tx2));
    let start = Instant::now();
    let mut output = String::new();
    let status = loop {
        if cancel.load(Ordering::Acquire) || ENDING.load(Ordering::Acquire) {
            return Err("AutoSetup.benchmarkCancelled".into());
        }
        if start.elapsed() > timeout {
            return Err("AutoSetup.benchmarkFailed".into());
        }
        if let Ok(line) = rx.recv_timeout(Duration::from_millis(30)) {
            progress(&line);
            if output.len() < 16 * 1024 * 1024 {
                output.push_str(&line);
                output.push('\n');
            }
        }
        if let Some(status) = process.child.lock().unwrap().try_wait().map_err(|e| e.to_string())? {
            break status;
        }
    };
    let _ = a.join();
    let _ = b.join();
    for line in rx {
        progress(&line);
        if output.len() < 16 * 1024 * 1024 {
            output.push_str(&line);
            output.push('\n');
        }
    }
    if !status.success() {
        return Err("AutoSetup.benchmarkFailed".into());
    }
    Ok(output)
}
fn start(app: AppHandle, shared: State, job: BenchmarkJob) {
    std::thread::spawn(move || {
        let result = if job.experimental {
            run_experimental(&app, &shared, &job)
        } else {
            run_official(&app, &shared, &job)
        };
        shared.workbench.lock().unwrap().complete_benchmark(job.id, result);
        crate::publish(&app, &shared);
    });
}
fn run_official(app: &AppHandle, shared: &State, job: &BenchmarkJob) -> Result<Observation, String> {
    let apple = cfg!(all(target_os = "macos", target_arch = "aarch64"))
        && shared
            .workbench
            .lock()
            .unwrap()
            .config()
            .engine
            .engines
            .iter()
            .any(|p| {
                p.command == job.target.command
                    && p.managed
                        .as_ref()
                        .is_some_and(|m| m.asset_id == "macos-arm64" && m.command == p.command)
            });
    let identity = if apple {
        let hardware = apple_identity(&job.cancel)?;
        let digest = tuning::apple_fingerprint(&job.target.command, &job.target.launch, &hardware, true)?;
        Some((hardware, digest))
    } else {
        None
    };
    let output = collect(
        &job.target.benchmark(5),
        &job.cancel,
        Duration::from_secs(30 * 60),
        |line| {
            if shared.workbench.lock().unwrap().benchmark_progress(job.id, line) {
                crate::publish(app, shared);
            }
        },
    )?;
    let mut observation = benchmark::parse(&output, 0);
    if let Some((hardware, digest)) = identity {
        observation.apple_hardware = Some(hardware);
        observation.apple_fingerprint = Some(digest);
    }
    Ok(observation)
}
/// Startup opt-in: delay, then wait up to Java's 45 seconds for the foreground to be ready.
pub fn startup(app: AppHandle, shared: State) {
    std::thread::spawn(move || {
        if cfg!(all(target_os = "macos", target_arch = "aarch64")) {
            let hardware = apple_identity(&AtomicBool::new(false)).ok();
            shared.workbench.lock().unwrap().set_apple_hardware(hardware);
            // A changed identity restarts an Apple-tuned foreground engine through a launch check.
            crate::check_launches(&app, &shared);
        }
        let apple = cfg!(all(target_os = "macos", target_arch = "aarch64"));
        let deadline = Instant::now() + Duration::from_secs(45);
        while !ENDING.load(Ordering::Acquire) && Instant::now() < deadline {
            let ready = shared.workbench.lock().unwrap().startup_engine_ready();
            if ready {
                std::thread::sleep(Duration::from_secs(if apple { 8 } else { 3 }));
                if ENDING.load(Ordering::Acquire) {
                    return;
                }
                let probe = shared.workbench.lock().unwrap().startup_apple_probe();
                let apple_match = probe.map(|(profile, roots, hardware)| {
                    let matched = tuning::apple_profile_matches(&profile, &roots, &hardware);
                    (profile.id, profile.command, matched)
                });
                let job = shared.workbench.lock().unwrap().begin_startup_benchmark(apple_match);
                if let Some(job) = job {
                    crate::publish(&app, &shared);
                    start(app, shared, job);
                }
                return;
            }
            std::thread::sleep(Duration::from_millis(100));
        }
    });
}
fn api_error(s: impl Into<String>) -> ApiError {
    ApiError::new(ErrorKind::InvalidArgument, s)
}
fn result(result: TuningResult) -> Result<CommandResult, ApiError> {
    Ok(CommandResult::Tuning { result })
}
/// Same chooser convention as the other native dialogs: system title, technical file filter.
fn chosen(app: &AppHandle, filter: Option<(&str, &[&str])>) -> Option<String> {
    let mut builder = app.dialog().file();
    if let Some((label, extensions)) = filter {
        builder = builder.add_filter(label, extensions);
    }
    builder
        .blocking_pick_file()
        .and_then(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
}
fn selected(shared: &State, id: &str) -> Result<(String, LaunchSpec), ApiError> {
    shared.workbench.lock().unwrap().gtp_configuration_target(id)
}
pub async fn dispatch(app: AppHandle, shared: State, op: TuningCommand) -> Result<CommandResult, ApiError> {
    match op {
        TuningCommand::Refresh => {
            // Java reads the benchmark target's model header off the UI thread for the B11 notice.
            let unread = shared.workbench.lock().unwrap().unread_weight_headers();
            let headers = tauri::async_runtime::spawn_blocking(move || {
                unread
                    .into_iter()
                    .filter_map(|path| {
                        let name = lizzie_core::resources::read_weight_model_name(std::path::Path::new(&path))?;
                        Some((path, name))
                    })
                    .collect::<BTreeMap<_, _>>()
            })
            .await
            .map_err(|e| api_error(e.to_string()))?;
            shared.workbench.lock().unwrap().record_weight_headers(headers);
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::SetStartupBenchmark { enabled } => {
            shared.workbench.lock().unwrap().set_startup_benchmark(enabled)?;
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::StartBenchmark {
            engine_id,
            experimental,
        } => {
            let job = shared
                .workbench
                .lock()
                .unwrap()
                .begin_benchmark(&engine_id, experimental)?;
            let task_id = job.id;
            crate::publish(&app, &shared);
            start(app, shared, job);
            result(TuningResult::Started { task_id })
        }
        TuningCommand::StopBenchmark => {
            shared.workbench.lock().unwrap().stop_benchmark();
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::DismissStartupNotice { task_id } => {
            shared.workbench.lock().unwrap().dismiss_startup_notice(task_id);
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::SetThreadSource { engine_id, source } => {
            shared.workbench.lock().unwrap().set_thread_source(&engine_id, source)?;
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::RestoreMeasured { engine_id } => {
            shared.workbench.lock().unwrap().restore_measured_tuning(&engine_id)?;
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        }
        TuningCommand::ImportMeasured { engine_id, path } => tauri::async_runtime::spawn_blocking(move || {
            let path = path.or_else(|| chosen(&app, Some(("*.json", &["json"]))));
            let Some(path) = path else {
                return result(TuningResult::Cancelled);
            };
            if std::fs::metadata(&path).map_err(|e| api_error(e.to_string()))?.len() > 1024 * 1024 {
                return Err(api_error("Report exceeds 1 MiB"));
            }
            let report: serde_json::Value =
                serde_json::from_str(&std::fs::read_to_string(&path).map_err(|e| api_error(e.to_string()))?)
                    .map_err(|e| api_error(e.to_string()))?;
            tuning::validate_measured_report(&report).map_err(api_error)?;
            let target = shared.workbench.lock().unwrap().measured_target(&engine_id)?;
            let (gpu, driver, memory) = gpu_identity().map_err(api_error)?;
            shared
                .workbench
                .lock()
                .unwrap()
                .set_measured_hardware(Some((gpu.clone(), driver.clone(), memory)));
            let fingerprint = tuning::measured_fingerprint(&target, &gpu, &driver, memory).map_err(api_error)?;
            if fingerprint != report["fingerprint"] {
                return Err(api_error(
                    "Engine, weights, config, GPU, driver or command no longer matches this report",
                ));
            }
            let review_id =
                shared
                    .workbench
                    .lock()
                    .unwrap()
                    .review_measured_tuning(&engine_id, &target.command, report.clone())?;
            result(TuningResult::MeasuredReview {
                engine_id,
                review_id,
                report,
            })
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
        TuningCommand::ConfirmMeasured { engine_id, review_id } => tauri::async_runtime::spawn_blocking(move || {
            let (target, report) = shared
                .workbench
                .lock()
                .unwrap()
                .measured_review(&engine_id, review_id)?;
            let (gpu, driver, memory) = gpu_identity().map_err(api_error)?;
            if tuning::measured_fingerprint(&target, &gpu, &driver, memory).map_err(api_error)? != report["fingerprint"]
            {
                return Err(api_error(
                    "Engine, weights, config, GPU, driver or command no longer matches this report",
                ));
            }
            shared
                .workbench
                .lock()
                .unwrap()
                .set_measured_hardware(Some((gpu, driver, memory)));
            shared
                .workbench
                .lock()
                .unwrap()
                .confirm_measured_tuning(&engine_id, review_id)?;
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
        TuningCommand::InspectGtpConfiguration { engine_id } => tauri::async_runtime::spawn_blocking(move || {
            let (_, spec) = selected(&shared, &engine_id)?;
            let mut session = GtpSession::open(&spec).map_err(api_error)?;
            let schema = gtp_config::inspect(&mut session).map_err(api_error)?;
            result(TuningResult::GtpSchema { engine_id, schema })
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
        TuningCommand::SaveGtpConfiguration {
            engine_id,
            configuration,
        } => tauri::async_runtime::spawn_blocking(move || {
            let (command, spec) = selected(&shared, &engine_id)?;
            if configuration.protocol != "zengtp-config" {
                return Err(api_error("Unsupported GTP configuration protocol"));
            }
            let mut session = GtpSession::open(&spec).map_err(api_error)?;
            let saved = gtp_config::apply(&mut session, &configuration.profile).map_err(api_error)?;
            shared
                .workbench
                .lock()
                .unwrap()
                .save_gtp_configuration(&engine_id, &command, saved)?;
            crate::publish(&app, &shared);
            result(TuningResult::Done)
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
        TuningCommand::ScanIkatago {
            platform,
            user_name,
            password,
        } => tauri::async_runtime::spawn_blocking(move || {
            if !matches!(platform.as_str(), "colab" | "all") {
                return Err(api_error("Invalid ikatago platform"));
            }
            let Some(exe) = chosen(&app, None) else {
                return result(TuningResult::Cancelled);
            };
            if [&user_name, &password].iter().any(|s| s.contains(['\r', '\n', '"'])) {
                return Err(api_error("Invalid ikatago credentials"));
            }
            result(TuningResult::Scanned {
                command: format!(
                    "\"{exe}\" --platform {platform} --username \"{user_name}\" --password \"{password}\""
                ),
            })
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
        TuningCommand::Scan { engine_type } => tauri::async_runtime::spawn_blocking(move || {
            let Some(exe) = chosen(&app, None) else {
                return result(TuningResult::Cancelled);
            };
            let q = |s: &str| format!("\"{}\"", s);
            let command = match engine_type.as_str() {
                "KataGo" => {
                    let Some(model) = chosen(&app, Some(("*.bin.gz / *.txt.gz", &["gz"]))) else {
                        return result(TuningResult::Cancelled);
                    };
                    let Some(config) = chosen(&app, Some(("*.cfg", &["cfg"]))) else {
                        return result(TuningResult::Cancelled);
                    };
                    format!("{} gtp -model {} -config {}", q(&exe), q(&model), q(&config))
                }
                "LeelaZero" => {
                    let Some(model) = chosen(&app, Some(("*.gz", &["gz"]))) else {
                        return result(TuningResult::Cancelled);
                    };
                    format!("{} -g -w {}", q(&exe), q(&model))
                }
                "ikatago" => return Err(api_error("ikatago requires platform and account selection")),
                _ => q(&exe),
            };
            result(TuningResult::Scanned { command })
        })
        .await
        .map_err(|e| api_error(e.to_string()))?,
    }
}
struct GtpSession {
    process: Process,
    stdin: ChildStdin,
    rx: mpsc::Receiver<String>,
    next: u64,
}
impl GtpSession {
    fn open(spec: &LaunchSpec) -> Result<Self, String> {
        let process = spawn(spec)?;
        let (stdin, stdout, stderr) = {
            let mut c = process.child.lock().unwrap();
            (
                c.stdin.take().ok_or("Missing stdin")?,
                c.stdout.take().ok_or("Missing stdout")?,
                c.stderr.take().ok_or("Missing stderr")?,
            )
        };
        let (tx, rx) = mpsc::channel();
        std::thread::spawn(move || {
            use std::io::{BufRead, BufReader};
            for line in BufReader::new(stdout).lines() {
                match line {
                    Ok(s) => {
                        if tx.send(s).is_err() {
                            break;
                        }
                    }
                    Err(_) => break,
                }
            }
        });
        std::thread::spawn(move || {
            let _ = std::io::copy(&mut std::io::BufReader::new(stderr), &mut std::io::sink());
        });
        Ok(Self {
            process,
            stdin,
            rx,
            next: 0,
        })
    }
}
impl gtp_config::Session for GtpSession {
    fn request(&mut self, command: &str) -> Result<String, String> {
        self.next += 1;
        writeln!(self.stdin, "{} {command}", self.next)
            .and_then(|_| self.stdin.flush())
            .map_err(|e| e.to_string())?;
        let deadline = Instant::now() + Duration::from_secs(12);
        let mut payload = String::new();
        let mut success = None;
        loop {
            let line = self
                .rx
                .recv_timeout(deadline.saturating_duration_since(Instant::now()))
                .map_err(|_| "GTP configuration probe timed out")?;
            if success.is_none() {
                if let Some(rest) = line.strip_prefix('=').or_else(|| line.strip_prefix('?')) {
                    let rest = rest.trim_start();
                    let digits = rest.chars().take_while(char::is_ascii_digit).count();
                    if rest[..digits].parse::<u64>().ok() != Some(self.next) {
                        continue;
                    }
                    success = Some(line.starts_with('='));
                    payload.push_str(rest[digits..].trim_start());
                }
            } else if line.trim().is_empty() {
                return if success == Some(true) {
                    Ok(payload)
                } else {
                    Err(payload)
                };
            } else {
                if !payload.is_empty() {
                    payload.push('\n');
                }
                payload.push_str(&line);
            }
        }
    }
}
impl Drop for GtpSession {
    fn drop(&mut self) {
        let _ = writeln!(self.stdin, "quit");
        let _ = self.stdin.flush();
        let _ = &self.process;
    }
}
fn gpu_identity() -> Result<(String, String, u64), String> {
    let spec = LaunchSpec {
        program: "nvidia-smi".into(),
        args: vec![
            "--query-gpu=name,driver_version,memory.total".into(),
            "--format=csv,noheader,nounits".into(),
        ],
        working_dir: None,
        library_dirs: Vec::new(),
    };
    let output = collect(&spec, &AtomicBool::new(false), Duration::from_secs(8), |_| {})?;
    let mut rows = output.lines().filter(|s| !s.trim().is_empty());
    let row = rows.next().ok_or("A single measured NVIDIA GPU is required")?;
    if rows.next().is_some() {
        return Err("A single measured NVIDIA GPU is required".into());
    }
    let columns: Vec<_> = row.split(',').map(str::trim).collect();
    if columns.len() != 3 {
        return Err("Incomplete GPU identity".into());
    }
    Ok((
        columns[0].into(),
        columns[1].into(),
        columns[2].parse().map_err(|_| "Incomplete GPU memory identity")?,
    ))
}
fn run_experimental(app: &AppHandle, shared: &State, job: &BenchmarkJob) -> Result<Observation, String> {
    let hardware = apple_identity(&job.cancel)?;
    shared
        .workbench
        .lock()
        .unwrap()
        .set_apple_hardware(Some(hardware.clone()));
    let fingerprint = tuning::apple_fingerprint(&job.target.command, &job.target.launch, &hardware, false)?;
    let mut seed = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map_or(0, |d| d.as_nanos() as u64)
        & ((1u64 << 48) - 1);
    let memory = collect(
        &LaunchSpec {
            program: "/usr/sbin/sysctl".into(),
            args: vec!["-n".into(), "hw.memsize".into()],
            working_dir: None,
            library_dirs: Vec::new(),
        },
        &job.cancel,
        Duration::from_secs(3),
        |_| {},
    )?
    .trim()
    .parse::<u64>()
    .map_err(|_| "Unknown Apple memory")?;
    let model = job.target.environment["model"]["size"].as_u64().unwrap_or(0);
    let mut candidates = benchmark::apple_candidates(memory, model);
    benchmark::shuffle(&mut candidates, &mut seed);
    let mut smoke = Vec::new();
    for candidate in candidates {
        if job.cancel.load(Ordering::Acquire) {
            return Err("AutoSetup.benchmarkCancelled".into());
        }
        let mut spec = job.target.benchmark(5);
        spec.args = benchmark::apple_benchmark_args(&spec.args, &candidate, true);
        let output = collect(&spec, &job.cancel, Duration::from_secs(600), |line| {
            shared
                .workbench
                .lock()
                .unwrap()
                .benchmark_progress(job.id, &format!("{} smoke: {line}", candidate.id));
            crate::publish(app, shared);
        });
        if let Ok(output) = output {
            let observation = benchmark::parse(&output, 6);
            if benchmark::apple_cell_valid(&candidate, &observation) {
                smoke.push((candidate, observation));
            }
        }
    }
    let mut shortlist = benchmark::apple_shortlist(smoke)?;
    benchmark::shuffle(&mut shortlist, &mut seed);
    let mut full = Vec::new();
    for candidate in shortlist {
        let mut spec = job.target.benchmark(5);
        spec.args = benchmark::apple_benchmark_args(&spec.args, &candidate, false);
        if let Ok(output) = collect(&spec, &job.cancel, Duration::from_secs(1800), |line| {
            shared
                .workbench
                .lock()
                .unwrap()
                .benchmark_progress(job.id, &format!("{} full: {line}", candidate.id));
            crate::publish(app, shared);
        }) {
            let result = benchmark::parse(&output, 0);
            if benchmark::apple_cell_valid(&candidate, &result)
                && (!candidate.devices.contains(&100) || result.recommended_threads >= 2)
            {
                full.push((candidate, result));
            }
        }
    }
    let mut samples: BTreeMap<String, Vec<Observation>> = BTreeMap::new();
    for round in 0..3 {
        benchmark::shuffle(&mut full, &mut seed);
        for (candidate, official) in &full {
            let mut spec = job.target.benchmark(5);
            spec.args = benchmark::apple_benchmark_args(&spec.args, candidate, false);
            spec.args.retain(|s| s != "-s");
            spec.args
                .extend(["-t".into(), official.recommended_threads.to_string()]);
            if let Ok(output) = collect(&spec, &job.cancel, Duration::from_secs(1800), |line| {
                shared.workbench.lock().unwrap().benchmark_progress(
                    job.id,
                    &format!("{} verification {}/3: {line}", candidate.id, round + 1),
                );
                crate::publish(app, shared);
            }) {
                let observation = benchmark::parse(&output, official.recommended_threads);
                if benchmark::apple_cell_valid(candidate, &observation) {
                    samples.entry(candidate.id.clone()).or_default().push(observation);
                }
            }
        }
    }
    let mut result = benchmark::apple_select(full, &samples)?;
    result.apple_hardware = Some(hardware);
    result.apple_fingerprint = Some(fingerprint);
    Ok(result)
}

/// Java `AppleSiliconHardwareProbe`: fixed facts, never an inferred chip or memory size.
fn apple_identity(cancel: &AtomicBool) -> Result<serde_json::Value, String> {
    let mut hardware = serde_json::Map::new();
    for (key, program, args) in [
        ("model", "/usr/sbin/sysctl", vec!["-n", "hw.model"]),
        ("chipName", "/usr/sbin/sysctl", vec!["-n", "machdep.cpu.brand_string"]),
        ("architecture", "/usr/sbin/sysctl", vec!["-n", "hw.machine"]),
        ("logicalCores", "/usr/sbin/sysctl", vec!["-n", "hw.logicalcpu"]),
        ("memoryBytes", "/usr/sbin/sysctl", vec!["-n", "hw.memsize"]),
        ("rosetta", "/usr/sbin/sysctl", vec!["-n", "sysctl.proc_translated"]),
        ("osBuild", "/usr/bin/sw_vers", vec!["-buildVersion"]),
    ] {
        let spec = LaunchSpec {
            program: program.into(),
            args: args.into_iter().map(String::from).collect(),
            working_dir: None,
            library_dirs: Vec::new(),
        };
        let value = collect(&spec, cancel, Duration::from_secs(3), |_| {});
        let value = if key == "rosetta" {
            value.unwrap_or_else(|_| "0".into())
        } else {
            value?
        };
        hardware.insert(key.into(), serde_json::Value::String(value.trim().to_string()));
    }
    Ok(serde_json::Value::Object(hardware))
}
