//! Tauri platform adapter. Owns windows, native dialogs, the clipboard, threads and event
//! delivery; every state change goes through the core [`Workbench`].

#[cfg(feature = "acceptance")]
mod acceptance;
mod diagnostics;
mod logging;
mod resources;
mod tuning;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use lizzie_core::api::{
    ApiError, Command, CommandResult, ConsoleLineView, ErrorKind, LogsStatus, OpenSource, WorkbenchView,
};
use lizzie_core::engine::process::{ProcessLauncher, ProcessWaiter};
use lizzie_core::formats;
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::settings::{Storage, WindowPlacement};
use lizzie_core::workbench::{OpenOrigin, PasteDecision, SaveJob, SavePlan, Workbench};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub const VIEW_EVENT: &str = "workbench-view";
/// A Java entry whose capability is not implemented yet was used; payload names the capability.
pub const UNAVAILABLE_EVENT: &str = "capability-unavailable";
/// New GTP console lines (`ConsoleLineView[]`).
pub const CONSOLE_EVENT: &str = "engine-console";
/// Engine output is published at most this often; user commands publish at once.
const ENGINE_REFRESH: Duration = Duration::from_millis(50);

pub struct Shared {
    pub workbench: Mutex<Workbench>,
    pub log: logging::Log,
    refresh: Mutex<(bool, bool)>,
    refresh_cv: Condvar,
    processes: ProcessWaiter,
    /// The one serial writer of record files (Java `SgfSaveQueue`: a single worker thread).
    saves: Mutex<Sender<SaveJob>>,
    /// The window asked to close while saves were running; it closes when they are done.
    close_after_saves: AtomicBool,
}

pub type State = Arc<Shared>;

fn write_logs(shared: &Shared, logs: Vec<lizzie_core::workbench::LogLine>) {
    for l in logs {
        if l.warn {
            shared.log.warn(&l.text);
        } else {
            shared.log.info(&l.text);
        }
    }
}

/// Publishes the next view. Built and emitted under the lock so views arrive in `seq` order.
pub fn publish(app: &AppHandle, shared: &Shared) {
    let mut wb = shared.workbench.lock().unwrap();
    let view = wb.view(false);
    let _ = app.emit(VIEW_EVENT, &view);
    let logs = wb.take_logs();
    drop(wb);
    write_logs(shared, logs);
}

impl Shared {
    /// Asks the refresher to publish engine-caused changes (view and/or console lines).
    fn request_refresh(&self, view: bool, console: bool) {
        let mut r = self.refresh.lock().unwrap();
        r.0 |= view;
        r.1 |= console;
        self.refresh_cv.notify_one();
    }
}

/// Coalesces engine output into at most one view (and one console batch) per
/// [`ENGINE_REFRESH`], so analysis streams do not flood the WebView.
fn run_refresher(app: AppHandle, shared: State) {
    std::thread::Builder::new()
        .name("engine-refresh".into())
        .spawn(move || {
            let mut last = Instant::now() - ENGINE_REFRESH;
            let mut console_seq = 0;
            loop {
                let (view, mut console) = {
                    // Taken before the refresh lock: other threads lock the workbench first.
                    let due = shared.workbench.lock().unwrap().engine_timer();
                    let mut r = shared.refresh.lock().unwrap();
                    while !r.0 && !r.1 {
                        match due {
                            None => r = shared.refresh_cv.wait(r).unwrap(),
                            Some(due) => {
                                let wait = due.saturating_duration_since(Instant::now());
                                let (next, timeout) = shared.refresh_cv.wait_timeout(r, wait).unwrap();
                                r = next;
                                if timeout.timed_out() {
                                    break;
                                }
                            }
                        }
                    }
                    drop(r);
                    std::thread::sleep(ENGINE_REFRESH.saturating_sub(last.elapsed()));
                    std::mem::take(&mut *shared.refresh.lock().unwrap())
                };
                last = Instant::now();
                {
                    // Timed engine work (a pondering probe) shows up as console output.
                    let mut wb = shared.workbench.lock().unwrap();
                    let before = wb.console_seq();
                    wb.engine_tick(last);
                    console |= wb.console_seq() != before;
                }
                if view {
                    publish(&app, &shared);
                }
                if console {
                    let lines: Vec<ConsoleLineView> = shared.workbench.lock().unwrap().console_since(console_seq);
                    if let Some(l) = lines.last() {
                        console_seq = l.seq;
                        let _ = app.emit(CONSOLE_EVENT, &lines);
                    }
                }
            }
        })
        .expect("engine refresh thread");
}

fn start_open(app: &AppHandle, shared: &State, path: String, source: OpenSource) -> u64 {
    let ticket = shared.workbench.lock().unwrap().begin_open(OpenOrigin::File {
        path: path.clone(),
        source,
    });
    publish(app, shared);
    let app = app.clone();
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = formats::read_file(&PathBuf::from(&path));
        if let Err(e) = &result {
            shared.log.warn(&format!(
                "open failed ({}): {}",
                logging::file_label(&path),
                logging::read_error(e)
            ));
        }
        let adopted = shared.workbench.lock().unwrap().complete_open(ticket, result);
        if adopted {
            publish(&app, &shared);
        } else {
            shared
                .log
                .info(&format!("open ticket {ticket} superseded; result dropped"));
        }
    });
    ticket
}

/// The `AP` value written into saved records.
fn application(app: &AppHandle) -> String {
    format!("LizzieYzy Tauri:{}", app.package_info().version)
}

/// Captures the current document and hands it to the save lane.
fn start_save(app: &AppHandle, shared: &State, selected: &str) -> CommandResult {
    let job = shared.workbench.lock().unwrap().begin_save(selected, &application(app));
    let ticket = job.ticket;
    shared.log.info(&format!(
        "save {ticket} started ({})",
        logging::file_label(&job.path.to_string_lossy())
    ));
    if let Err(mpsc::SendError(job)) = shared.saves.lock().unwrap().send(job) {
        finish_save(app, shared, job.ticket, Err("save worker stopped".into()));
    }
    CommandResult::SaveStarted { ticket }
}

fn finish_save(app: &AppHandle, shared: &Shared, ticket: u64, result: Result<(), String>) {
    match &result {
        Ok(()) => shared.log.info(&format!("save {ticket} written")),
        Err(e) => shared.log.warn(&format!("save {ticket} failed: {e}")),
    }
    let idle = {
        let mut wb = shared.workbench.lock().unwrap();
        wb.complete_save(ticket, result);
        !wb.has_pending_saves()
    };
    publish(app, shared);
    if idle && shared.close_after_saves.swap(false, Ordering::SeqCst) {
        if let Some(window) = app.get_webview_window("main") {
            let _ = window.close();
        }
    }
}

fn spawn_save_lane(app: AppHandle) -> Sender<SaveJob> {
    let (tx, rx) = mpsc::channel::<SaveJob>();
    std::thread::Builder::new()
        .name("lizzie-sgf-save".into())
        .spawn(move || {
            for job in rx {
                let result = formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
                let shared = app.state::<State>().inner().clone();
                finish_save(&app, &shared, job.ticket, result);
            }
        })
        .expect("save worker thread");
    tx
}

/// "另存为": the system save chooser in the last folder with the suggested name. The chooser
/// confirms replacing the file the user picked; Java's own prompt remains for the case where
/// `.sgf` is appended and that file exists (`SgfSaveCoordinator.chooseAndSave`).
async fn choose_and_save(app: &AppHandle, shared: &State, suggested_name: String) -> Result<CommandResult, ApiError> {
    let last_folder = shared.workbench.lock().unwrap().persisted().last_folder.clone();
    // Without a usable last folder Java's `JFileChooser("")` starts in the platform default
    // directory: Documents on Windows, the home directory elsewhere.
    let start_dir = last_folder.map(PathBuf::from).filter(|d| d.is_dir()).or_else(|| {
        if cfg!(windows) {
            app.path().document_dir().ok()
        } else {
            app.path().home_dir().ok()
        }
    });
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        // Java filter: FileNameExtensionFilter("*.sgf", "SGF").
        let mut builder = dialog
            .file()
            .add_filter("*.sgf", &["sgf", "SGF"])
            .set_file_name(suggested_name);
        if let Some(dir) = start_dir {
            builder = builder.set_directory(dir);
        }
        builder.blocking_save_file()
    })
    .await
    .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
    let Some(selected) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(CommandResult::Unchanged);
    };
    let selected = selected.to_string_lossy().into_owned();
    let target = formats::save_target(&selected);
    if target != selected && std::path::Path::new(&target).exists() {
        return Ok(CommandResult::NeedsOverwriteConfirmation { path: target });
    }
    Ok(start_save(app, shared, &target))
}

fn changed(app: &AppHandle, shared: &Shared, changed: bool) -> CommandResult {
    if changed {
        publish(app, shared);
        CommandResult::Done
    } else {
        CommandResult::Unchanged
    }
}

/// The one entry for every command, used by the UI and by acceptance automation.
pub async fn dispatch(app: AppHandle, shared: State, command: Command) -> Result<CommandResult, ApiError> {
    match command {
        Command::ChooseAndOpenRecord => {
            let start_dir = shared.workbench.lock().unwrap().persisted().last_folder.clone();
            let dialog = app.dialog().clone();
            let picked = tauri::async_runtime::spawn_blocking(move || {
                // Java filter: "SGF/GIB (*.sgf, *.gib)" (LizzieFrame.java:4628, 4642).
                let mut builder = dialog
                    .file()
                    .add_filter("SGF/GIB (*.sgf, *.gib)", &["sgf", "gib", "SGF", "GIB"]);
                if let Some(dir) = start_dir.filter(|d| std::path::Path::new(d).is_dir()) {
                    builder = builder.set_directory(dir);
                }
                builder.blocking_pick_file()
            })
            .await
            .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
            match picked.and_then(|p| p.into_path().ok()) {
                Some(path) => {
                    let ticket = start_open(&app, &shared, path.to_string_lossy().into_owned(), OpenSource::Chooser);
                    Ok(CommandResult::OpenStarted { ticket })
                }
                None => Ok(CommandResult::Unchanged),
            }
        }
        Command::OpenRecord { path, source } => {
            let ticket = start_open(&app, &shared, path, source);
            Ok(CommandResult::OpenStarted { ticket })
        }
        Command::PasteRecord { confirmed } => {
            let text = app.clipboard().read_text().unwrap_or_default();
            let decision = shared.workbench.lock().unwrap().paste_decision(&text, confirmed);
            match decision {
                PasteDecision::Ignore => Ok(CommandResult::Unchanged),
                PasteDecision::Confirm => Ok(CommandResult::NeedsConfirmation),
                PasteDecision::Proceed => {
                    let ticket = shared.workbench.lock().unwrap().begin_open(OpenOrigin::Clipboard);
                    let result = Workbench::read_clipboard_record(&text);
                    if let Err(e) = &result {
                        shared
                            .log
                            .warn(&format!("clipboard record failed: {}", logging::read_error(e)));
                    }
                    shared.workbench.lock().unwrap().complete_open(ticket, result);
                    publish(&app, &shared);
                    Ok(CommandResult::OpenStarted { ticket })
                }
            }
        }
        Command::SaveRecord {
            confirmed,
            suggested_name,
        } => {
            let plan = shared.workbench.lock().unwrap().save_plan(confirmed);
            match plan {
                SavePlan::Choose => choose_and_save(&app, &shared, suggested_name).await,
                SavePlan::Confirm { document, file_name } => {
                    Ok(CommandResult::NeedsReplaceConfirmation { document, file_name })
                }
                SavePlan::Overwrite { path } => Ok(start_save(&app, &shared, &path)),
            }
        }
        Command::ChooseAndSaveRecord { suggested_name } => choose_and_save(&app, &shared, suggested_name).await,
        Command::SaveRecordTo { path } => Ok(start_save(&app, &shared, &path)),
        Command::SetReplaceFileHint { enabled } => {
            let done = shared.workbench.lock().unwrap().set_replace_file_hint(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::Navigate { nav } => {
            let moved = shared.workbench.lock().unwrap().navigate(nav);
            Ok(changed(&app, &shared, moved))
        }
        Command::GotoNode { node } => {
            let moved = shared.workbench.lock().unwrap().goto_node(node)?;
            Ok(changed(&app, &shared, moved))
        }
        Command::GotoMoveNumber { number } => {
            let moved = shared.workbench.lock().unwrap().goto_move_number(number);
            Ok(changed(&app, &shared, moved))
        }
        Command::SetReadKomi { enabled } => {
            let done = shared.workbench.lock().unwrap().set_read_komi(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetLanguage { language } => {
            let done = shared.workbench.lock().unwrap().set_language(language)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetPlaySound { enabled } => {
            let done = shared.workbench.lock().unwrap().set_play_sound(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::ClearPersonalData => {
            let done = shared.workbench.lock().unwrap().clear_personal_data()?;
            shared.log.info("personal data cleared");
            Ok(changed(&app, &shared, done))
        }
        Command::ResetUiPosition => {
            shared.workbench.lock().unwrap().reset_ui_position()?;
            shared.log.info("ui position reset");
            // The re-enabled prompts show in the settings menu.
            Ok(changed(&app, &shared, true))
        }
        Command::OpenLogFolder => {
            let dir = shared.workbench.lock().unwrap().storage().logs_dir.clone();
            std::fs::create_dir_all(&dir).map_err(|e| ApiError::new(ErrorKind::Io, e.to_string()))?;
            app.opener()
                .open_path(dir.to_string_lossy(), None::<&str>)
                .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
            Ok(CommandResult::Done)
        }
        Command::ExportDiagnostics => {
            let dialog = app.dialog().clone();
            let picked = tauri::async_runtime::spawn_blocking(move || {
                dialog
                    .file()
                    .add_filter("ZIP", &["zip"])
                    .set_file_name(diagnostics::default_file_name())
                    .blocking_save_file()
            })
            .await
            .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
            let Some(path) = picked.and_then(|p| p.into_path().ok()) else {
                return Ok(CommandResult::Unchanged);
            };
            let package = {
                let wb = shared.workbench.lock().unwrap();
                diagnostics::Package::collect(&wb, &app.package_info().version.to_string())
            };
            package
                .write(&path)
                .map_err(|e| ApiError::new(ErrorKind::Io, e.to_string()))?;
            shared.log.info("diagnostics package exported");
            Ok(CommandResult::Exported {
                path: path.to_string_lossy().into_owned(),
            })
        }
        Command::ToggleAnalysis => {
            let done = shared.workbench.lock().unwrap().toggle_analysis()?;
            Ok(changed(&app, &shared, done))
        }
        Command::SwitchEngine { index } => {
            let done = shared.workbench.lock().unwrap().switch_engine(index)?;
            Ok(changed(&app, &shared, done))
        }
        Command::RestartEngine => {
            let done = shared.workbench.lock().unwrap().restart_engine()?;
            Ok(changed(&app, &shared, done))
        }
        Command::ShutdownEngines { scope } => {
            let done = shared.workbench.lock().unwrap().shutdown_engines(scope);
            Ok(changed(&app, &shared, done))
        }
        Command::SaveEngineSettings { engines, startup } => {
            let done = shared
                .workbench
                .lock()
                .unwrap()
                .save_engine_settings(engines, startup)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetAnalysisSettings { settings } => {
            let done = shared.workbench.lock().unwrap().set_analysis_settings(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetKomi { komi } => {
            let done = shared.workbench.lock().unwrap().set_komi(komi)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetKataRules { rules, auto_load } => {
            let refused = shared.workbench.lock().unwrap().set_kata_rules(&rules, auto_load)?;
            publish(&app, &shared);
            Ok(match refused {
                None => CommandResult::Done,
                Some(key) => CommandResult::Refused {
                    message_key: key.to_string(),
                },
            })
        }
        Command::SendGtpCommand { command } => {
            let refused = shared.workbench.lock().unwrap().send_gtp(&command)?;
            publish(&app, &shared);
            Ok(match refused {
                None => CommandResult::Done,
                Some("") => CommandResult::Unchanged,
                Some(key) => CommandResult::Refused {
                    message_key: key.to_string(),
                },
            })
        }
        Command::SetFullTrace { enabled } => {
            let done = shared.workbench.lock().unwrap().set_full_trace(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetPonderLimitTips { enabled } => {
            let done = shared.workbench.lock().unwrap().set_ponder_limit_tips(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::AnswerRulesPrompt { prompt, accept } => {
            let done = shared.workbench.lock().unwrap().answer_rules_prompt(prompt, accept);
            Ok(changed(&app, &shared, done))
        }
        Command::ChooseJavaFolder { program } => {
            let dialog = app.dialog().clone();
            let picked = tauri::async_runtime::spawn_blocking(move || {
                let builder = dialog.file();
                let builder = if program {
                    builder
                } else {
                    // Start where Java keeps its settings when one of those folders exists.
                    match java_locations().into_iter().find(|d| d.is_dir()) {
                        Some(dir) => builder.set_directory(dir),
                        None => builder,
                    }
                };
                builder.blocking_pick_folder()
            })
            .await
            .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
            Ok(match picked.and_then(|p| p.into_path().ok()) {
                Some(path) => CommandResult::FolderChosen {
                    path: path.to_string_lossy().into_owned(),
                },
                None => CommandResult::Unchanged,
            })
        }
        Command::PreviewJavaImport {
            config_dir,
            program_dir,
        } => {
            let source = JavaSource::new(
                std::path::Path::new(&config_dir),
                program_dir.as_deref().map(std::path::Path::new),
            );
            // Reading stays off the workbench lock and the UI thread.
            let read = tauri::async_runtime::spawn_blocking(move || java_import::read(&source))
                .await
                .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
            match read {
                Ok(candidate) => {
                    let preview = shared.workbench.lock().unwrap().preview_java_import(candidate);
                    Ok(CommandResult::JavaImportPreview { preview })
                }
                Err(failure) => {
                    shared.log.warn(&format!("java import: cannot read {failure:?}"));
                    Ok(CommandResult::JavaImportFailed { failure })
                }
            }
        }
        Command::ApplyJavaImport {
            preview,
            copy_credentials,
            replace,
        } => {
            let result = shared
                .workbench
                .lock()
                .unwrap()
                .apply_java_import(preview, &copy_credentials, replace);
            let result = match result {
                Ok(r) => r,
                Err(e) => {
                    shared.log.warn(&format!("java import failed: {}", e.message));
                    return Err(e);
                }
            };
            publish(&app, &shared);
            Ok(match result {
                Ok(report) => CommandResult::JavaImported { report },
                Err(key) => CommandResult::Refused {
                    message_key: key.to_string(),
                },
            })
        }
        Command::DismissJavaImportOffer => {
            let done = shared.workbench.lock().unwrap().dismiss_java_import_offer()?;
            Ok(changed(&app, &shared, done))
        }
        Command::Resource { op } => resources::dispatch(app, shared, op).await,
        Command::Tuning { op } => tuning::dispatch(app, shared, op).await,
    }
}

/// Java's conventional work directories on this system (`WorkDirectoryResolver`; `user.home` is
/// `HOME`, on Windows `USERPROFILE`).
fn java_locations() -> Vec<PathBuf> {
    let var = |k: &str| std::env::var_os(k).filter(|v| !v.is_empty()).map(PathBuf::from);
    if cfg!(windows) {
        java_import::conventional_dirs(
            var("USERPROFILE").as_deref(),
            var("PUBLIC").as_deref(),
            var("PROGRAMDATA").as_deref(),
        )
    } else {
        java_import::conventional_dirs(var("HOME").as_deref(), None, None)
    }
}

/// GtpConsolePane: console lines after `since` (0 = everything kept).
#[tauri::command]
fn engine_console(state: tauri::State<'_, State>, since: u64) -> Vec<ConsoleLineView> {
    state.workbench.lock().unwrap().console_since(since)
}

/// Where relative engine paths are looked up, in Java's order: the current directory, then the
/// application's location and up to seven ancestors (`CommandLaunchHelper.collectCandidateRoots`).
fn engine_roots() -> Vec<PathBuf> {
    let mut roots = Vec::new();
    if let Ok(cwd) = std::env::current_dir() {
        roots.push(cwd);
    }
    if let Some(dir) = std::env::current_exe().ok().and_then(|e| e.parent().map(PathBuf::from)) {
        for a in dir.ancestors().take(8) {
            if !roots.iter().any(|r| r == a) {
                roots.push(a.to_path_buf());
            }
        }
    }
    roots
}

#[tauri::command]
async fn dispatch_command(
    app: AppHandle,
    state: tauri::State<'_, State>,
    command: Command,
) -> Result<CommandResult, ApiError> {
    dispatch(app, state.inner().clone(), command).await
}

/// Full view including the variation tree (UI start, reload).
#[tauri::command]
fn workbench_snapshot(state: tauri::State<'_, State>) -> WorkbenchView {
    state.workbench.lock().unwrap().view(true)
}

#[tauri::command]
fn logs_directory(state: tauri::State<'_, State>) -> LogsStatus {
    state.workbench.lock().unwrap().logs_status()
}

fn window_placement(window: &tauri::WebviewWindow, previous: Option<WindowPlacement>) -> Option<WindowPlacement> {
    let maximized = window.is_maximized().ok()?;
    if maximized {
        // Java keeps the normal bounds of a maximized window (`noneMax`).
        return previous.map(|p| WindowPlacement { maximized: true, ..p });
    }
    let pos = window.outer_position().ok()?;
    let size = window.inner_size().ok()?;
    Some(WindowPlacement {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            let config_dir = app.path().app_config_dir()?;
            let data_dir = app.path().app_data_dir()?;
            let storage = Storage::in_dirs(&config_dir, &data_dir);
            let log = logging::Log::start(&storage.logs_dir);
            log.info(&format!(
                "start {} {} on {}/{}",
                app.package_info().name,
                app.package_info().version,
                std::env::consts::OS,
                std::env::consts::ARCH
            ));
            let locale = sys_locale::get_locale();
            // Engine output reaches the workbench through the shared state, which exists only
            // after the workbench: the sink looks it up once it is set.
            let cell: Arc<OnceLock<State>> = Arc::new(OnceLock::new());
            let sink_cell = cell.clone();
            let launcher = ProcessLauncher::new(Arc::new(move |instance, input| {
                let Some(shared) = sink_cell.get() else { return };
                let mut wb = shared.workbench.lock().unwrap();
                let console = wb.console_seq();
                let view = wb.engine_input(instance, input);
                let console = wb.console_seq() != console;
                drop(wb);
                if view || console {
                    shared.request_refresh(view, console);
                }
            }));
            let processes = launcher.waiter();
            let mut workbench = Workbench::new(storage, locale.as_deref(), Box::new(launcher), engine_roots());
            workbench.set_java_import_locations(java_locations());
            let placement = workbench.persisted().window;
            let shared: State = Arc::new(Shared {
                workbench: Mutex::new(workbench),
                log,
                refresh: Mutex::new((false, false)),
                refresh_cv: Condvar::new(),
                processes,
                saves: Mutex::new(spawn_save_lane(app.handle().clone())),
                close_after_saves: AtomicBool::new(false),
            });
            let _ = cell.set(shared.clone());
            app.manage(shared.clone());
            run_refresher(app.handle().clone(), shared.clone());
            shared.workbench.lock().unwrap().start_engines();
            publish(app.handle(), &shared);

            let window = app.get_webview_window("main").expect("main window");
            if let Some(p) = placement {
                let _ = window.set_size(PhysicalSize::new(p.width, p.height));
                let _ = window.set_position(PhysicalPosition::new(p.x, p.y));
                if p.maximized {
                    let _ = window.maximize();
                }
            }

            // Command line: one argument other than "read" opens that record without touching
            // the recent list (Lizzie.java:1049-1052).
            let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
            if let [path] = args.as_slice() {
                if path != "read" {
                    start_open(app.handle(), &shared, path.clone(), OpenSource::CommandLine);
                }
            }

            #[cfg(feature = "acceptance")]
            acceptance::start(app.handle().clone(), shared.clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { api, .. } => {
                let Some(webview) = window.app_handle().get_webview_window(window.label()) else {
                    return;
                };
                let shared = window.app_handle().state::<State>().inner().clone();
                let mut wb = shared.workbench.lock().unwrap();
                // Accepted writes finish before exit (Lizzie.shutdown waits for pendingSaves); the
                // save lane closes the window when the last one completes.
                if wb.has_pending_saves() {
                    shared.close_after_saves.store(true, Ordering::SeqCst);
                    api.prevent_close();
                    shared.log.info("exit waits for running saves");
                    return;
                }
                let previous = wb.persisted().window;
                if let Err(e) = wb.persist_on_exit(window_placement(&webview, previous)) {
                    shared.log.warn(&e.message);
                }
                shared.log.info("exit");
            }
            WindowEvent::DragDrop(tauri::DragDropEvent::Drop { paths, .. }) => {
                let app = window.app_handle();
                let shared = app.state::<State>().inner().clone();
                match paths.as_slice() {
                    // A single dropped file opens like the command line (LizzieFrame.java:4048).
                    [path] => {
                        start_open(app, &shared, path.to_string_lossy().into_owned(), OpenSource::Drop);
                    }
                    [] => {}
                    // Several files start batch analysis in Java, which is not available yet.
                    _ => {
                        let _ = app.emit(UNAVAILABLE_EVENT, "batchAnalysis");
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(handlers())
        .build(tauri::generate_context!())
        .expect("error while building LizzieYzy Tauri")
        .run(|app, event| {
            if let RunEvent::Exit = event {
                // Every exit path (window close, 退出, 强制退出) ends the engines and waits for
                // their processes, so none is left behind.
                let shared = app.state::<State>().inner().clone();
                shared.workbench.lock().unwrap().end_engines();
                shared.processes.wait(Duration::from_secs(4));
                let logs = shared.workbench.lock().unwrap().take_logs();
                write_logs(&shared, logs);
                shared.log.info("engines ended");
            }
        });
}

#[cfg(not(feature = "acceptance"))]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![dispatch_command, workbench_snapshot, logs_directory, engine_console]
}

#[cfg(feature = "acceptance")]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dispatch_command,
        workbench_snapshot,
        logs_directory,
        engine_console,
        acceptance::acceptance_ui_report
    ]
}
