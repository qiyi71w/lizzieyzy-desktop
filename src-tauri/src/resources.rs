//! Native I/O adapter: all network, filesystem discovery and helper processes run off-lock.
use crate::State;
use lizzie_core::{
    api::{ApiError, CommandResult, ErrorKind, ResourceCommand, ResourceEngineChoice, ResourceResult},
    resources as r,
    settings::{NetworkSettings, ProxyMode},
    workbench::{prepare_resource_setup, run_resource_job, ResourceJob},
};
use std::{
    io::{self, Read},
    path::{Path, PathBuf},
    process::{Command, Stdio},
    sync::{
        atomic::{AtomicBool, Ordering},
        Arc, LazyLock, Mutex, Weak,
    },
    thread,
    time::{Duration, Instant},
};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

fn error(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorKind::Io, lizzie_core::sanitize::sanitize(&e.to_string()))
}
fn response(result: ResourceResult) -> CommandResult {
    CommandResult::Resource { result }
}
fn refused(key: &str) -> ResourceResult {
    ResourceResult::Refused {
        message_key: key.into(),
    }
}
static TOKENS: LazyLock<Mutex<Vec<Weak<AtomicBool>>>> = LazyLock::new(|| Mutex::new(Vec::new()));
static CHILDREN: LazyLock<Mutex<Vec<Weak<Mutex<std::process::Child>>>>> = LazyLock::new(|| Mutex::new(Vec::new()));
fn register(cancel: &Arc<AtomicBool>) {
    let mut tokens = TOKENS.lock().unwrap();
    tokens.retain(|t| t.strong_count() > 0);
    tokens.push(Arc::downgrade(cancel));
}
/// Every helper process polls these tokens, kills and reaps its own child on exit.
pub fn shutdown() {
    for token in TOKENS.lock().unwrap().iter().filter_map(Weak::upgrade) {
        token.store(true, Ordering::Release);
    }
    for child in CHILDREN.lock().unwrap().iter().filter_map(Weak::upgrade) {
        let mut child = child.lock().unwrap();
        let _ = child.kill();
        let _ = child.wait();
    }
}

/// Redirecting HTTP client with one captured proxy setting for the whole task.
pub struct HttpFetch {
    client: reqwest::blocking::Client,
}
impl HttpFetch {
    pub fn new(settings: &NetworkSettings) -> r::ResourceResult<Self> {
        let mut b = reqwest::blocking::Client::builder()
            .user_agent(concat!("LizzieYzy-Tauri/", env!("CARGO_PKG_VERSION")))
            .connect_timeout(Duration::from_secs(15))
            .timeout(Duration::from_secs(120))
            .redirect(reqwest::redirect::Policy::limited(10));
        match settings.proxy_mode {
            ProxyMode::Direct => b = b.no_proxy(),
            ProxyMode::System => {}
            ProxyMode::Manual => {
                let host = settings.proxy_host.trim();
                if host.is_empty() || settings.proxy_port == 0 {
                    return Err(r::ResourceError::Invalid("Invalid manual proxy".into()));
                }
                let host = if host.contains(':') && !host.starts_with('[') {
                    format!("[{host}]")
                } else {
                    host.into()
                };
                b = b
                    .no_proxy()
                    .proxy(reqwest::Proxy::http(format!("http://{host}:{}", settings.proxy_port)).map_err(http_error)?)
                    .proxy(
                        reqwest::Proxy::https(format!("http://{host}:{}", settings.proxy_port)).map_err(http_error)?,
                    );
            }
        }
        Ok(Self {
            client: b.build().map_err(http_error)?,
        })
    }
}
fn http_error(e: reqwest::Error) -> r::ResourceError {
    r::ResourceError::Invalid(e.without_url().to_string())
}
impl r::Fetch for HttpFetch {
    fn get(&self, url: &str, start: Option<u64>) -> r::ResourceResult<r::FetchResponse> {
        let url = download_url(url);
        let mut request = self.client.get(url);
        if let Some(offset) = start {
            request = request.header(reqwest::header::RANGE, format!("bytes={offset}-"));
        }
        let result = request.send().map_err(http_error)?;
        let status = result.status().as_u16();
        let mut total = result.content_length();
        if status == 206 {
            let range = result
                .headers()
                .get(reqwest::header::CONTENT_RANGE)
                .and_then(|h| h.to_str().ok())
                .and_then(|s| s.strip_prefix("bytes "))
                .and_then(|s| s.split_once('/'));
            let (span, length) = range.ok_or_else(|| r::ResourceError::Invalid("Missing Content-Range".into()))?;
            let (first, last) = span
                .split_once('-')
                .ok_or_else(|| r::ResourceError::Invalid("Invalid Content-Range".into()))?;
            let first = first
                .parse::<u64>()
                .map_err(|_| r::ResourceError::Invalid("Invalid range start".into()))?;
            let last = last
                .parse::<u64>()
                .map_err(|_| r::ResourceError::Invalid("Invalid range end".into()))?;
            let length = length
                .parse::<u64>()
                .map_err(|_| r::ResourceError::Invalid("Invalid range total".into()))?;
            if first != start.unwrap_or(0)
                || last < first
                || last >= length
                || total.is_some_and(|n| n != last - first + 1)
            {
                return Err(r::ResourceError::Invalid("Mismatched Content-Range".into()));
            }
            total = Some(length);
        }
        Ok(r::FetchResponse {
            status,
            total_length: total,
            body: Box::new(result),
        })
    }
}
#[cfg(feature = "acceptance")]
fn download_url(url: &str) -> String {
    if let Ok(base) = std::env::var("LIZZIEYZY_TAURI_RESOURCE_BASE") {
        // Preserve filenames for local fault/slow-download fixtures; never changes pinned digests.
        if (url.starts_with("https://github.com/") && url.contains("/releases/download/"))
            || url.starts_with(r::OFFICIAL_MODEL_PREFIX)
        {
            if let Some(name) = url.rsplit('/').next() {
                return format!("{}/{name}", base.trim_end_matches('/'));
            }
        }
    }
    url.into()
}
#[cfg(not(feature = "acceptance"))]
fn download_url(url: &str) -> String {
    url.into()
}

struct CancelProbe {
    base: r::StdProbe,
    cancel: Arc<AtomicBool>,
}
impl r::Probe for CancelProbe {
    fn run(&self, program: &str, args: &[String], timeout: Duration) -> io::Result<r::ProbeOutput> {
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
        }
        let path = Path::new(program);
        let executable = if path.components().count() > 1 {
            std::fs::canonicalize(path)?
        } else {
            path.into()
        };
        let mut command = Command::new(&executable);
        command
            .args(args)
            .stdin(Stdio::null())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped());
        if let Some(parent) = executable.parent().filter(|p| !p.as_os_str().is_empty()) {
            command.current_dir(parent);
        }
        #[cfg(windows)]
        {
            use std::os::windows::process::CommandExt;
            command.creation_flags(0x08000000);
            let mut paths = self.base.library_dirs.clone();
            if let Some(p) = std::env::var_os("PATH") {
                paths.extend(std::env::split_paths(&p));
            }
            command.env("PATH", std::env::join_paths(paths).map_err(io::Error::other)?);
        }
        let mut children = CHILDREN.lock().unwrap();
        if self.cancel.load(Ordering::Acquire) {
            return Err(io::Error::new(io::ErrorKind::Interrupted, "Cancelled"));
        }
        let mut child = command.spawn()?;
        let readers = [
            child.stdout.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
            child.stderr.take().map(|s| Box::new(s) as Box<dyn Read + Send>),
        ]
        .into_iter()
        .flatten()
        .map(|mut stream| {
            thread::spawn(move || {
                let mut saved = Vec::new();
                let mut buffer = [0; 8192];
                while let Ok(n) = stream.read(&mut buffer) {
                    if n == 0 {
                        break;
                    }
                    let keep = n.min((4 * 1024 * 1024usize).saturating_sub(saved.len()));
                    saved.extend_from_slice(&buffer[..keep]);
                }
                String::from_utf8_lossy(&saved).into_owned()
            })
        })
        .collect::<Vec<_>>();
        let child = Arc::new(Mutex::new(child));
        children.retain(|p| p.strong_count() > 0);
        children.push(Arc::downgrade(&child));
        drop(children);
        let started = Instant::now();
        let mut timed_out = false;
        let status = loop {
            let mut process = child.lock().unwrap();
            if let Some(status) = process.try_wait()? {
                break status;
            }
            if self.cancel.load(Ordering::Acquire) || started.elapsed() >= timeout {
                timed_out = started.elapsed() >= timeout;
                let _ = process.kill();
                break process.wait()?;
            }
            drop(process);
            thread::sleep(Duration::from_millis(25));
        };
        let mut texts = readers.into_iter().map(|h| h.join().unwrap_or_default());
        Ok(r::ProbeOutput {
            exit_code: status.code(),
            stdout: texts.next().unwrap_or_default(),
            stderr: texts.next().unwrap_or_default(),
            timed_out,
        })
    }
    fn read_file(&self, path: &Path) -> io::Result<Vec<u8>> {
        r::Probe::read_file(&self.base, path)
    }
    fn list_dir(&self, path: &Path) -> io::Result<Vec<PathBuf>> {
        r::Probe::list_dir(&self.base, path)
    }
    fn env_var(&self, key: &str) -> Option<String> {
        r::Probe::env_var(&self.base, key)
    }
    fn logical_cpus(&self) -> Option<u32> {
        r::Probe::logical_cpus(&self.base)
    }
    fn cpu_avx2_fma(&self) -> Option<bool> {
        r::Probe::cpu_avx2_fma(&self.base)
    }
    fn read_link(&self, path: &Path) -> io::Result<PathBuf> {
        r::Probe::read_link(&self.base, path)
    }
}
async fn choose(app: &AppHandle, kind: &str, start: Option<PathBuf>) -> Result<Option<String>, ApiError> {
    let dialog = app.dialog().clone();
    let kind = kind.to_string();
    tauri::async_runtime::spawn_blocking(move || {
        let mut b = dialog.file();
        b = match kind.as_str() {
            "weight" => b.add_filter("*.bin.gz / *.txt.gz", &["gz"]),
            "package" => b.add_filter("*.zip", &["zip"]),
            "config" => b.add_filter("*.cfg", &["cfg"]),
            _ => {
                #[cfg(windows)]
                {
                    b = b.add_filter("KataGo executable", &["exe", "bat"]);
                }
                b
            }
        };
        if let Some(start) = start {
            b = b.set_directory(start);
        }
        b.blocking_pick_file()
            .and_then(|p| p.into_path().ok())
            .map(|p| p.to_string_lossy().into_owned())
    })
    .await
    .map_err(error)
}
async fn complete_local_choice(
    app: &AppHandle,
    mut c: lizzie_core::api::LocalEngineChoice,
) -> Result<Option<lizzie_core::api::LocalEngineChoice>, ApiError> {
    if c.executable.is_none() {
        c.executable = choose(app, "executable", None).await?;
    }
    let Some(executable) = &c.executable else {
        return Ok(None);
    };
    let parent = Path::new(executable).parent().map(Path::to_path_buf);
    let directory = parent.clone();
    let found = tauri::async_runtime::spawn_blocking(move || {
        let mut config = None;
        let mut weight = None;
        if let Some(dir) = directory {
            for name in ["gtp.cfg", "default_gtp.cfg"] {
                let p = dir.join(name);
                if p.is_file() {
                    config = Some(p.to_string_lossy().into_owned());
                    break;
                }
            }
            if let Ok(entries) = std::fs::read_dir(dir) {
                weight = entries
                    .filter_map(Result::ok)
                    .map(|e| e.path())
                    .filter(|p| p.is_file())
                    .find(|p| {
                        let s = p.to_string_lossy();
                        s.ends_with(".bin.gz") || s.ends_with(".txt.gz")
                    })
                    .map(|p| p.to_string_lossy().into_owned());
            }
        }
        (config, weight)
    })
    .await
    .map_err(error)?;
    c.config = c.config.or(found.0);
    c.weight = c.weight.or(found.1);
    if c.config.is_none() {
        c.config = choose(app, "config", parent.clone()).await?;
    }
    if c.config.is_none() {
        return Ok(None);
    }
    if c.weight.is_none() {
        c.weight = choose(app, "weight", parent).await?;
    }
    if c.weight.is_none() {
        return Ok(None);
    }
    Ok(Some(c))
}
fn launch(app: AppHandle, shared: State, job: ResourceJob, network: NetworkSettings) {
    register(&job.cancel);
    thread::spawn(move || {
        let probe = CancelProbe {
            base: r::StdProbe::with_library_dirs(vec![job.store.runtime_dir()]),
            cancel: job.cancel.clone(),
        };
        let mut last = Instant::now() - Duration::from_secs(1);
        let result = HttpFetch::new(&network).and_then(|fetch| {
            run_resource_job(&job, &fetch, &probe, |phase, bytes, total| {
                if last.elapsed() >= Duration::from_millis(120) || total == Some(bytes) {
                    let changed = shared
                        .workbench
                        .lock()
                        .unwrap()
                        .resource_progress(job.id, phase, bytes, total);
                    if changed {
                        crate::publish(&app, &shared);
                    }
                    last = Instant::now();
                }
            })
        });
        let result = if job.cancel.load(Ordering::Acquire) && result.is_err() {
            Err(r::ResourceError::Cancelled)
        } else {
            result
        };
        let changed = shared.workbench.lock().unwrap().complete_resource_task(job.id, result);
        if changed {
            crate::publish(&app, &shared);
        }
    });
}
/// Startup detection only: no download, verification, benchmark or profile mutation.
pub fn startup(app: AppHandle, shared: State) {
    let started = {
        let mut wb = shared.workbench.lock().unwrap();
        let network = wb.resource_network();
        wb.begin_resource_task(ResourceCommand::Inspect { refresh: false })
            .map(|job| (job, network))
    };
    if let Ok((job, network)) = started {
        launch(app, shared, job, network);
    }
}

pub async fn dispatch(app: AppHandle, shared: State, mut op: ResourceCommand) -> Result<CommandResult, ApiError> {
    match op {
        ResourceCommand::CancelTask { task } => {
            let result = shared.workbench.lock().unwrap().cancel_resource_task(task);
            crate::publish(&app, &shared);
            return Ok(response(result));
        }
        ResourceCommand::SetNetworkProxy { settings } => {
            let result = shared.workbench.lock().unwrap().set_resource_proxy(settings)?;
            crate::publish(&app, &shared);
            return Ok(response(result));
        }
        ResourceCommand::ApplySetup {
            plan,
            make_default,
            switch_now,
        } => {
            let result = shared
                .workbench
                .lock()
                .unwrap()
                .apply_resource_setup(plan, make_default, switch_now)?;
            crate::publish(&app, &shared);
            return Ok(response(result));
        }
        ResourceCommand::PlanSetup { engine, weight } => {
            let version = shared
                .workbench
                .lock()
                .unwrap()
                .resource_verified_version(&engine.executable);
            let choice = tauri::async_runtime::spawn_blocking(move || prepare_resource_setup(engine, weight, version))
                .await
                .map_err(error)?
                .map_err(error)?;
            let result = shared.workbench.lock().unwrap().plan_resource_setup(choice);
            return Ok(response(result));
        }
        ResourceCommand::OpenResourceFolder => {
            let root = shared.workbench.lock().unwrap().resource_store().root().to_path_buf();
            let path = tauri::async_runtime::spawn_blocking(move || {
                std::fs::create_dir_all(&root)?;
                Ok::<_, io::Error>(root.to_string_lossy().into_owned())
            })
            .await
            .map_err(error)?
            .map_err(error)?;
            app.opener().open_path(path, None::<&str>).map_err(error)?;
            return Ok(response(ResourceResult::Done));
        }
        ResourceCommand::CleanNvidiaCache { confirmed: false } => {
            let store = shared.workbench.lock().unwrap().resource_store();
            let bytes = tauri::async_runtime::spawn_blocking(move || store.runtime_cache_size())
                .await
                .map_err(error)?
                .map_err(error)?;
            return Ok(response(ResourceResult::CacheSize { bytes }));
        }
        ResourceCommand::ChooseLocalEngine { choice } => {
            let Some(choice) = complete_local_choice(&app, choice).await? else {
                return Ok(response(ResourceResult::Cancelled));
            };
            op = ResourceCommand::ChooseLocalEngine { choice };
        }
        ResourceCommand::ImportPackage {
            path,
            backend,
            confirmed,
        } => {
            let path = match path {
                Some(p) => Some(p),
                None => choose(&app, "package", None).await?,
            };
            let Some(path) = path else {
                return Ok(response(ResourceResult::Cancelled));
            };
            op = ResourceCommand::ImportPackage {
                path: Some(path),
                backend,
                confirmed,
            };
        }
        ResourceCommand::ImportWeight { path } => {
            let path = match path {
                Some(p) => Some(p),
                None => choose(&app, "weight", None).await?,
            };
            let Some(path) = path else {
                return Ok(response(ResourceResult::Cancelled));
            };
            op = ResourceCommand::ImportWeight { path: Some(path) };
        }
        ResourceCommand::EnableTensorRt | ResourceCommand::SwitchBackToCuda => {
            if !cfg!(windows) {
                return Ok(response(refused("AutoSetup.tensorRtNotApplicable")));
            }
            let (engine, weight, version) = {
                let tensor = matches!(op, ResourceCommand::EnableTensorRt);
                let wb = shared.workbench.lock().unwrap();
                let v = wb.resource_snapshot();
                let asset = if tensor { "windows-tensorrt" } else { "windows-nvidia" };
                let Some(package) = v
                    .inventory
                    .packages
                    .iter()
                    .find(|p| v.packages.iter().any(|a| a.id == asset && p.is_current(a)))
                else {
                    return Ok(response(refused("AutoSetup.tensorRtEngineMissing")));
                };
                let Some(weight) = v.current_setup.weight.clone() else {
                    return Ok(response(refused("AutoSetup.tensorRtMissingWeight")));
                };
                let engine = ResourceEngineChoice {
                    asset_id: asset.into(),
                    executable: package.executable.to_string_lossy().into_owned(),
                    config: v
                        .inventory
                        .gtp_config
                        .as_ref()
                        .map(|p| p.to_string_lossy().into_owned())
                        .unwrap_or_default(),
                    backend: None,
                    update_profile_id: if v.current_setup.managed {
                        v.current_setup.profile_id
                    } else {
                        None
                    },
                };
                let version = wb.resource_verified_version(&engine.executable);
                (engine, weight, version)
            };
            let choice = tauri::async_runtime::spawn_blocking(move || prepare_resource_setup(engine, weight, version))
                .await
                .map_err(error)?
                .map_err(error)?;
            return Ok(response(shared.workbench.lock().unwrap().plan_resource_setup(choice)));
        }
        _ => {}
    }
    let started = {
        let mut wb = shared.workbench.lock().unwrap();
        let network = wb.resource_network();
        wb.begin_resource_task(op).map(|job| (job, network))
    };
    match started {
        Err(result) => Ok(response(*result)),
        Ok((job, network)) => {
            let task = job.id;
            crate::publish(&app, &shared);
            launch(app, shared, job, network);
            Ok(response(ResourceResult::Started { task }))
        }
    }
}
