//! Tauri platform adapter. Owns windows, native dialogs, the clipboard, threads and event
//! delivery; every state change goes through the core [`Workbench`].

#[cfg(feature = "acceptance")]
mod acceptance;
mod diagnostics;
mod independent;
mod logging;
mod output;
mod resources;
mod startup;
mod tuning;

use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{self, Sender};
use std::sync::{Arc, Condvar, Mutex, OnceLock};
use std::time::{Duration, Instant};

use lizzie_core::api::{
    ApiError, BatchCommand, BatchMode, Command, CommandResult, ConsoleLineView, ErrorKind, LogsStatus, OpenSource,
    WorkbenchView,
};
use lizzie_core::engine::process::{ProcessLauncher, ProcessWaiter};
use lizzie_core::formats;
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::settings::{FramePlacement, Persisted, Storage, WindowPlacement};
use lizzie_core::workbench::{OpenOrigin, PasteDecision, SaveJob, SavePlan, Workbench};
use tauri::ipc::Channel;
use tauri::{
    AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, RunEvent, WebviewUrl, WebviewWindowBuilder,
    WindowEvent,
};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

/// A Java entry whose capability is not implemented yet was used; payload names the capability.
pub const UNAVAILABLE_EVENT: &str = "capability-unavailable";
/// New GTP console lines (`ConsoleLineView[]`).
pub const CONSOLE_EVENT: &str = "engine-console";
/// Engine output is published at most this often; user commands publish at once.
const ENGINE_REFRESH: Duration = Duration::from_millis(50);
/// The 独立选点列表 (`AnalysisFrame`) window.
const SUGGESTIONS_WINDOW: &str = "suggestions";
/// The secondary engine's 独立选点列表 in 双引擎模式 (`AnalysisFrame(2)`, 副引擎).
const SUGGESTIONS_SUB_WINDOW: &str = "suggestions-sub";
/// The 独立选点列表 was closed by its own close box; the main window records it as closed.
pub const SUGGESTIONS_CLOSED_EVENT: &str = "suggestions-frame-closed";
/// 超级鹰眼 (`MoveListFrame(1)`).
const HAWKEYE_WINDOW: &str = "hawkeye";
/// 双引擎模式's secondary engine 超级鹰眼 (`MoveListFrame(2)`).
const HAWKEYE_SUB_WINDOW: &str = "hawkeye-sub";
/// 批量分析进度表 (`AnalysisTable`).
const BATCH_WINDOW: &str = "batch-table";
/// A 超级鹰眼 window was closed by its own close box; the main window toggles it off.
pub const HAWKEYE_CLOSED_EVENT: &str = "hawkeye-closed";

pub struct Shared {
    pub workbench: Mutex<Workbench>,
    pub log: logging::Log,
    /// Engine-caused view change, console lines, and a timer the refresher must re-read.
    refresh: Mutex<(bool, bool, bool)>,
    refresh_cv: Condvar,
    processes: ProcessWaiter,
    /// The one serial writer of record files (Java `SgfSaveQueue`: a single worker thread).
    saves: Mutex<Sender<SaveJob>>,
    /// An exit was requested during a save; close the main window after all writes finish.
    close_after_saves: AtomicBool,
    /// The exit automatic save (自动存档) of the window being closed.
    exit_save: Mutex<output::ExitSave>,
    views: Mutex<ViewSubscription>,
    tables: Mutex<TableLayout>,
}

/// Column widths and the 独立选点列表's bounds as the UI last laid them out; written on exit
/// (`Config.persist`: `main-window-list`, `suggestions-list-position-7/9`).
struct TableLayout {
    /// The main window's 选点列表, `None` while its score column is hidden.
    main_list: Option<Vec<u32>>,
    /// The open frame's column widths (7 or 9).
    frame_columns: Vec<u32>,
    /// Placements per layout as the frame last had them this session (Java rereads the
    /// in-memory `persistedUi` that closing the frame updates).
    frame_7: Option<FramePlacement>,
    frame_9: Option<FramePlacement>,
    /// The frame was opened this session: its last placement is written on exit.
    frame_last: Option<FramePlacement>,
    /// 超级鹰眼's sort column and 详细列表 column widths as its window last reported them
    /// (`badmoves-list-position[0]`, `[5..]`).
    hawkeye: (i32, Vec<u32>),
}

impl TableLayout {
    fn new(persisted: &Persisted) -> TableLayout {
        TableLayout {
            main_list: persisted.main_list_columns.clone(),
            frame_columns: Vec::new(),
            frame_7: persisted.suggestions_frame_7.clone(),
            frame_9: persisted.suggestions_frame_9.clone(),
            frame_last: None,
            hawkeye: persisted
                .hawkeye
                .as_ref()
                .map_or((2, Vec::new()), |p| (p.sort, p.columns.clone())),
        }
    }

    /// Records where the open frame is (closing it, or on exit).
    fn capture_frame(&mut self, window: &tauri::WebviewWindow) {
        let Some(placement) = frame_placement(window, &self.frame_columns) else {
            return;
        };
        if placement.columns.len() == 9 {
            self.frame_9 = Some(placement.clone());
        } else {
            self.frame_7 = Some(placement.clone());
        }
        self.frame_last = Some(placement);
    }
}

/// Logical bounds (Java pixels) with the inner size the frame is created with again.
fn frame_placement(window: &tauri::WebviewWindow, columns: &[u32]) -> Option<FramePlacement> {
    let scale = window.scale_factor().ok()?;
    let pos = window.outer_position().ok()?.to_logical::<i32>(scale);
    let size = window.inner_size().ok()?.to_logical::<u32>(scale);
    Some(FramePlacement {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        columns: columns.to_vec(),
    })
}

#[derive(Default)]
struct ViewSubscription {
    generation: u64,
    channel: Option<Channel<WorkbenchView>>,
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

/// Builds and sends each view under the workbench lock, preserving `seq` order. Before the window
/// subscribes no view is built, so one-shot parts (a command-line open's outcome) reach it with
/// its first snapshot.
pub fn publish(app: &AppHandle, shared: &Shared) {
    let mut wb = shared.workbench.lock().unwrap();
    let mut views = shared.views.lock().unwrap();
    if let Some(channel) = &views.channel {
        if channel.send(wb.view(false)).is_err() {
            views.channel = None;
        }
    }
    drop(views);
    let logs = wb.take_logs();
    let checks = wb.take_launch_checks();
    let auto_saves = wb.take_background_saves();
    let batch_reads = wb.take_batch_reads();
    drop(wb);
    write_logs(shared, logs);
    run_launch_checks(app, checks);
    for job in auto_saves {
        if let Err(mpsc::SendError(job)) = shared.saves.lock().unwrap().send(job) {
            finish_save(app, shared, job.ticket, Err("save worker stopped".into()));
        }
    }
    for (ticket, path, no_capture) in batch_reads {
        read_batch_record(app, ticket, path, no_capture);
    }
    // A command can schedule timed work (quick analysis waits for browsing to settle).
    shared.rearm_timer();
}

/// Engine starts whose overlays need artifact fingerprints (measured or Apple tuning) are hashed
/// on their own thread, never under the workbench lock (Java `requireWorker`).
pub fn check_launches(app: &AppHandle, shared: &Shared) {
    let checks = shared.workbench.lock().unwrap().take_launch_checks();
    run_launch_checks(app, checks);
}

fn run_launch_checks(app: &AppHandle, checks: Vec<lizzie_core::engine::runtime::LaunchCheck>) {
    for check in checks {
        let app = app.clone();
        std::thread::spawn(move || {
            let checked = check.run();
            let shared = app.state::<State>().inner().clone();
            shared.workbench.lock().unwrap().finish_launch(checked);
            publish(&app, &shared);
        });
    }
}

impl Shared {
    /// Asks the refresher to publish engine-caused changes (view and/or console lines).
    fn request_refresh(&self, view: bool, console: bool) {
        let mut r = self.refresh.lock().unwrap();
        r.0 |= view;
        r.1 |= console;
        self.refresh_cv.notify_one();
    }

    /// Makes the refresher re-read [`Workbench::engine_timer`].
    fn rearm_timer(&self) {
        self.refresh.lock().unwrap().2 = true;
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
                let (mut view, mut console) = {
                    // Taken before the refresh lock: other threads lock the workbench first.
                    let due = shared.workbench.lock().unwrap().engine_timer();
                    let mut r = shared.refresh.lock().unwrap();
                    let mut due_now = false;
                    while !r.0 && !r.1 && !r.2 {
                        match due {
                            None => r = shared.refresh_cv.wait(r).unwrap(),
                            Some(due) => {
                                let wait = due.saturating_duration_since(Instant::now());
                                let (next, timeout) = shared.refresh_cv.wait_timeout(r, wait).unwrap();
                                r = next;
                                if timeout.timed_out() {
                                    due_now = true;
                                    break;
                                }
                            }
                        }
                    }
                    r.2 = false;
                    if !r.0 && !r.1 && !due_now {
                        continue; // only the timer changed: wait for the new one
                    }
                    drop(r);
                    std::thread::sleep(ENGINE_REFRESH.saturating_sub(last.elapsed()));
                    let mut r = shared.refresh.lock().unwrap();
                    (std::mem::take(&mut r.0), std::mem::take(&mut r.1))
                };
                last = Instant::now();
                {
                    // Timed engine work (a pondering probe) shows up as console output; quick
                    // analysis timers can change the view.
                    let mut wb = shared.workbench.lock().unwrap();
                    let before = wb.console_seq();
                    view |= wb.engine_tick(last);
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
    read_record(app, shared, ticket, PathBuf::from(path));
    ticket
}

/// Reads record file `path` on its own thread and hands it to the open request `ticket`
/// (files, 存档与读档 slots).
pub(crate) fn read_record(app: &AppHandle, shared: &State, ticket: u64, path: PathBuf) {
    let no_capture = shared.workbench.lock().unwrap().config().edit.no_capture;
    publish(app, shared);
    let app = app.clone();
    let shared = shared.clone();
    std::thread::spawn(move || {
        let result = formats::read_file(&path, no_capture);
        if let Err(e) = &result {
            shared.log.warn(&format!(
                "open failed ({}): {}",
                logging::file_label(&path.to_string_lossy()),
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
}

/// Reads a queued batch record on its own thread and hands it to the batch.
fn read_batch_record(app: &AppHandle, ticket: u64, path: PathBuf, no_capture: bool) {
    let app = app.clone();
    std::thread::spawn(move || {
        let shared = app.state::<State>().inner().clone();
        let result = formats::read_file(&path, no_capture);
        if let Err(e) = &result {
            shared.log.warn(&format!(
                "batch read failed ({}): {}",
                logging::file_label(&path.to_string_lossy()),
                logging::read_error(e)
            ));
        }
        if shared.workbench.lock().unwrap().complete_batch_read(ticket, result) {
            publish(&app, &shared);
        }
    });
}

/// The system chooser for several records (`LizzieFrame.chooseKifuFiles(…, true)`): the SGF/GIB
/// filter in the last folder. Empty when cancelled.
async fn choose_records(app: &AppHandle, shared: &State) -> Result<Vec<String>, ApiError> {
    let start_dir = shared.workbench.lock().unwrap().persisted().last_folder.clone();
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut builder = dialog
            .file()
            .add_filter("SGF/GIB (*.sgf, *.gib)", &["sgf", "gib", "SGF", "GIB"]);
        if let Some(dir) = start_dir.filter(|d| std::path::Path::new(d).is_dir()) {
            builder = builder.set_directory(dir);
        }
        builder.blocking_pick_files()
    })
    .await
    .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
    Ok(picked
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect())
}

fn batch_result(app: &AppHandle, shared: &Shared, result: Result<bool, String>) -> CommandResult {
    match result {
        Ok(done) => changed(app, shared, done),
        Err(message_key) => {
            publish(app, shared);
            CommandResult::Refused { message_key }
        }
    }
}

/// The `AP` value written into saved records.
pub(crate) fn application(app: &AppHandle) -> String {
    format!("LizzieYzy Tauri:{}", app.package_info().version)
}

/// Captures the current document and hands it to the save lane.
fn start_save(app: &AppHandle, shared: &State, selected: &str) -> CommandResult {
    let job = shared.workbench.lock().unwrap().begin_save(selected, &application(app));
    queue_write(app, shared, job)
}

/// Hands a captured save or export to the serial writer.
pub(crate) fn queue_write(app: &AppHandle, shared: &State, job: SaveJob) -> CommandResult {
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
                let result = (|| {
                    if job.create_parent {
                        if let Some(parent) = job.path.parent() { std::fs::create_dir_all(parent)?; }
                    }
                    formats::write_record_file(&job.path, &job.text)
                })().map_err(|e: std::io::Error| e.to_string());
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
    let start_dir = chooser_start(app, last_folder);
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

/// Where a save chooser starts: `folder` when it still exists, else Java's `JFileChooser("")`
/// platform default directory: Documents on Windows, the home directory elsewhere.
pub(crate) fn chooser_start(app: &AppHandle, folder: Option<String>) -> Option<PathBuf> {
    folder.map(PathBuf::from).filter(|d| d.is_dir()).or_else(|| {
        if cfg!(windows) {
            app.path().document_dir().ok()
        } else {
            app.path().home_dir().ok()
        }
    })
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
    let result = run_command(app.clone(), shared.clone(), command).await;
    // A command may start an engine without changing the view.
    check_launches(&app, &shared);
    result
}

async fn run_command(app: AppHandle, shared: State, command: Command) -> Result<CommandResult, ApiError> {
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
                    let (ticket, no_capture) = {
                        let mut wb = shared.workbench.lock().unwrap();
                        (wb.begin_open(OpenOrigin::Clipboard), wb.config().edit.no_capture)
                    };
                    let result = Workbench::read_clipboard_record(&text, no_capture);
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
        Command::ExportRecord {
            kind,
            suggested_name,
            labels,
        } => output::export_record(&app, &shared, kind, suggested_name, labels).await,
        Command::ExportRecordTo { kind, path, labels } => Ok(output::start_export(&app, &shared, kind, &path, &labels)),
        Command::CopyRecord => output::copy_record(&app, &shared),
        Command::ExportImage { png } => output::export_image(&app, &shared, png).await,
        Command::ExportImageTo { path, png } => output::export_image_to(&shared, path, png).await,
        Command::CopyImage { png } => output::copy_image(&app, &shared, &png),
        Command::Recovery { op } => output::recovery(&app, &shared, op),
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
        Command::SetAppendWinrateToComment { enabled } => {
            let done = shared
                .workbench
                .lock()
                .unwrap()
                .set_append_winrate_to_comment(enabled)?;
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
        Command::SetExtraMode { mode } => {
            let done = shared.workbench.lock().unwrap().set_extra_mode(mode)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetClassicMode => {
            let done = shared.workbench.lock().unwrap().set_classic_mode()?;
            Ok(changed(&app, &shared, done))
        }
        Command::ToggleIndependentBoard { board } => {
            let done = shared.workbench.lock().unwrap().toggle_independent_board(board)?;
            Ok(changed(&app, &shared, done))
        }
        Command::CloseIndependentBoard { board } => {
            let done = {
                let mut wb = shared.workbench.lock().unwrap();
                independent::capture(&app, &mut wb)?;
                wb.close_independent_board(board)?
            };
            Ok(changed(&app, &shared, done))
        }
        Command::SaveCustomLayout { slot, window } => {
            let done = {
                let mut wb = shared.workbench.lock().unwrap();
                independent::capture(&app, &mut wb)?;
                wb.save_custom_layout(slot, window)?
            };
            Ok(changed(&app, &shared, done))
        }
        Command::ApplyCustomLayout { slot } => {
            let done = shared.workbench.lock().unwrap().apply_custom_layout(slot)?;
            if done { independent::apply(&app, &shared, slot).await?; }
            Ok(changed(&app, &shared, done))
        }
        Command::SwitchSecondaryEngine { index } => {
            let done = shared.workbench.lock().unwrap().switch_secondary_engine(index)?;
            Ok(changed(&app, &shared, done))
        }
        Command::RestartSecondaryEngine => {
            let done = shared.workbench.lock().unwrap().restart_secondary_engine()?;
            Ok(changed(&app, &shared, done))
        }
        Command::ShutdownSecondaryEngine => {
            let done = shared.workbench.lock().unwrap().shutdown_secondary_engine();
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
            let result = shared.workbench.lock().unwrap().set_komi(komi);
            match result {
                Err(error) if error.message == "WholeGameAnalysis.conflict.analysis" =>
                    Ok(changed_refused(&app, &shared, error.message)),
                other => Ok(changed(&app, &shared, other?)),
            }
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
            recovery,
        } => {
            let result = shared.workbench.lock().unwrap().apply_java_import(
                preview,
                &copy_credentials,
                replace,
                recovery.as_ref(),
            );
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
        Command::Edit { op } => {
            let result = shared.workbench.lock().unwrap().edit(op)?;
            publish(&app, &shared);
            Ok(result)
        }
        Command::SetEditSettings { settings } => {
            let done = shared.workbench.lock().unwrap().set_edit_settings(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetDisplaySettings { settings } => {
            let done = shared.workbench.lock().unwrap().set_display_settings(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::TogglePanel { panel } => {
            let done = shared.workbench.lock().unwrap().toggle_panel(panel)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetPanelSettings { settings } => {
            let done = shared.workbench.lock().unwrap().set_panel_settings(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetPanelSizes { sizes } => {
            let done = shared.workbench.lock().unwrap().set_panel_sizes(sizes)?;
            Ok(changed(&app, &shared, done))
        }
        Command::NudgeBoardPosition { delta } => {
            let done = shared.workbench.lock().unwrap().nudge_board_position(delta)?;
            Ok(changed(&app, &shared, done))
        }
        Command::RestoreDefaultPanelSizes => {
            let done = shared.workbench.lock().unwrap().restore_default_panel_sizes()?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetBottomToolbar { mode } => {
            let done = shared.workbench.lock().unwrap().set_bottom_toolbar(mode)?;
            Ok(changed(&app, &shared, done))
        }
        Command::ToggleRawNet => {
            let done = shared.workbench.lock().unwrap().toggle_raw_net();
            Ok(changed(&app, &shared, done))
        }
        Command::StopRawNet => {
            let done = shared.workbench.lock().unwrap().stop_raw_net();
            Ok(changed(&app, &shared, done))
        }
        Command::SetKataEstimate { mode } => {
            let done = shared.workbench.lock().unwrap().set_kata_estimate(mode)?;
            Ok(changed(&app, &shared, done))
        }
        Command::ToggleKataEstimate => {
            let done = shared.workbench.lock().unwrap().toggle_kata_estimate();
            Ok(changed(&app, &shared, done))
        }
        Command::Score { op } => {
            let done = shared.workbench.lock().unwrap().score(op)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetAutoQuickAnalyze { enabled } => {
            let done = shared.workbench.lock().unwrap().set_auto_quick_analyze(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetLoadSgfLast { enabled } => {
            let done = shared.workbench.lock().unwrap().set_load_sgf_last(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SaveAnalysisEngineSettings {
            settings,
            shown_command,
            edited,
        } => {
            let saved =
                shared
                    .workbench
                    .lock()
                    .unwrap()
                    .save_analysis_engine_settings(settings, &shown_command, edited)?;
            Ok(match saved {
                Ok(done) => changed(&app, &shared, done),
                Err(key) => CommandResult::Refused {
                    message_key: key.to_string(),
                },
            })
        }
        Command::GenerateAnalysisCommand { index } => {
            let generated = shared.workbench.lock().unwrap().generate_analysis_command(index);
            Ok(match generated {
                Ok((command, generated_config)) => CommandResult::AnalysisCommand {
                    command,
                    generated_config,
                },
                Err(key) => CommandResult::Refused {
                    message_key: key.to_string(),
                },
            })
        }
        Command::SetQuickModelEnabled { enabled } => {
            let done = shared.workbench.lock().unwrap().set_quick_model_enabled(enabled)?;
            Ok(changed(&app, &shared, done))
        }
        Command::SetProblemList { settings } => {
            let done = shared.workbench.lock().unwrap().set_problem_list(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::JumpToProblemMove { node } => {
            let done = shared.workbench.lock().unwrap().jump_to_problem_move(node)?;
            Ok(changed(&app, &shared, done))
        }
        Command::StartFlashAnalysis { target } => {
            let started = shared.workbench.lock().unwrap().start_flash_analysis(target)?;
            Ok(match started {
                Ok(done) => changed(&app, &shared, done),
                Err(message_key) => changed_refused(&app, &shared, message_key),
            })
        }
        Command::CancelFlashAnalysis => {
            let done = shared.workbench.lock().unwrap().cancel_flash_analysis();
            Ok(changed(&app, &shared, done))
        }
        Command::RedoFlashAnalysis => {
            let started = shared.workbench.lock().unwrap().redo_flash_analysis()?;
            Ok(match started {
                Ok(done) => changed(&app, &shared, done),
                Err(message_key) => changed_refused(&app, &shared, message_key),
            })
        }
        Command::StartAutoAnalysis { settings } => {
            let result = shared.workbench.lock().unwrap().start_auto_analysis(settings)?;
            Ok(match result {
                Ok(done) => changed(&app, &shared, done),
                Err(key) => changed_refused(&app, &shared, key.into()),
            })
        }
        Command::StopAutoAnalysis => {
            let done = shared.workbench.lock().unwrap().stop_auto_analysis();
            Ok(changed(&app, &shared, done))
        }
        Command::AnswerAutoAnalysisDiff { accept } => {
            let done = shared.workbench.lock().unwrap().answer_auto_analysis_diff(accept);
            Ok(changed(&app, &shared, done))
        }
        Command::SaveAutoAnalysisSettings { settings } => {
            let refusal = shared.workbench.lock().unwrap().save_auto_analysis_settings(settings)?;
            Ok(match refusal {
                Some(key) => CommandResult::Refused { message_key: key.into() },
                None => changed(&app, &shared, true),
            })
        }
        Command::DismissAutoAnalysisTip { never_again } => {
            let done = shared.workbench.lock().unwrap().dismiss_auto_analysis_tip(never_again)?;
            Ok(changed(&app, &shared, done))
        }
        Command::StartWholeGameAnalysis { visits } => {
            let started = shared.workbench.lock().unwrap().start_whole_game_analysis(visits)?;
            Ok(match started {
                Ok(done) => changed(&app, &shared, done),
                Err(message_key) => changed_refused(&app, &shared, message_key),
            })
        }
        Command::PauseWholeGameAnalysis => {
            let done = shared.workbench.lock().unwrap().pause_whole_game_analysis();
            Ok(changed(&app, &shared, done))
        }
        Command::ResumeWholeGameAnalysis => {
            let done = shared.workbench.lock().unwrap().resume_whole_game_analysis();
            Ok(changed(&app, &shared, done))
        }
        Command::StopWholeGameAnalysis => {
            let done = shared.workbench.lock().unwrap().stop_whole_game_analysis();
            Ok(changed(&app, &shared, done))
        }
        Command::EstimateWholeGameAnalysis { visits } => {
            let millis = shared.workbench.lock().unwrap().estimate_whole_game_analysis(visits);
            Ok(CommandResult::WholeGameEstimate { millis })
        }
        Command::Hawkeye { op } => {
            let done = shared.workbench.lock().unwrap().hawkeye(op)?;
            Ok(changed(&app, &shared, done))
        }
        Command::Batch { op: BatchCommand::Choose { mode } } => {
            let paths = choose_records(&app, &shared).await?;
            if paths.is_empty() {
                return Ok(CommandResult::Unchanged);
            }
            let result = shared.workbench.lock().unwrap().open_batch(mode, paths, OpenSource::Chooser);
            Ok(batch_result(&app, &shared, result.map_err(str::to_string)))
        }
        Command::Batch { op: BatchCommand::AddFiles } => {
            let paths = choose_records(&app, &shared).await?;
            let done = shared.workbench.lock().unwrap().add_batch_files(paths);
            Ok(changed(&app, &shared, done))
        }
        Command::Batch { op } => {
            let result = shared.workbench.lock().unwrap().batch_command(op)?;
            Ok(batch_result(&app, &shared, result))
        }
        Command::SetHawkeyeSettings { settings } => {
            let done = shared.workbench.lock().unwrap().set_hawkeye_settings(settings)?;
            Ok(changed(&app, &shared, done))
        }
        Command::ExportTable { rows } => export_table(&app, rows).await,
    }
}

/// 导出为xls (`MoveListFrame` export listener, `Utils.exportTable`): Java's chooser has no title
/// and only the `(*.xls)` filter; the file is replaced without asking and a name containing
/// `.xlsx` writes nothing.
async fn export_table(app: &AppHandle, rows: Vec<Vec<String>>) -> Result<CommandResult, ApiError> {
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        dialog.file().add_filter("(*.xls)", &["xls"]).blocking_save_file()
    })
    .await
    .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
    let Some(chosen) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(CommandResult::Unchanged);
    };
    let Some(path) = formats::table::xls_target(&chosen.to_string_lossy()) else {
        return Ok(CommandResult::Unchanged);
    };
    let bytes = formats::table::encode(&rows, formats::table::NEWLINE);
    Ok(match std::fs::write(&path, bytes) {
        Ok(()) => CommandResult::Exported { path },
        Err(e) => CommandResult::ExportFailed {
            path,
            detail: e.to_string(),
        },
    })
}

/// A refusal after which state may still have changed (a quick analysis handed over, a range
/// saved): the view is published and the refusal answered.
fn changed_refused(app: &AppHandle, shared: &State, message_key: String) -> CommandResult {
    changed(app, shared, true);
    CommandResult::Refused { message_key }
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

/// Immutable catalog search does not acquire the workbench lock or read mutable application state.
#[tauri::command]
fn search_functions(query: String, language: lizzie_core::settings::Language) -> Vec<lizzie_core::function_search::FunctionMatch> {
    lizzie_core::function_search::search(&query, language)
}

/// Channels transfer large views as IPC data instead of compiling a new JS object literal.
#[tauri::command]
fn subscribe_workbench(state: tauri::State<'_, State>, channel: Channel<WorkbenchView>) -> u64 {
    let mut views = state.views.lock().unwrap();
    views.generation += 1;
    views.channel = Some(channel);
    views.generation
}

#[tauri::command]
fn unsubscribe_workbench(state: tauri::State<'_, State>, generation: u64) {
    let mut views = state.views.lock().unwrap();
    if views.generation == generation {
        views.channel = None;
    }
}

#[tauri::command]
fn logs_directory(state: tauri::State<'_, State>) -> LogsStatus {
    state.workbench.lock().unwrap().logs_status()
}

/// Java Overview's full-release action; no caller-provided URL is accepted.
#[tauri::command]
fn open_katago_releases(app: AppHandle) -> Result<(), ApiError> {
    app.opener()
        .open_url("https://github.com/lightvector/KataGo/releases", None::<&str>)
        .map_err(|_| ApiError::new(ErrorKind::Unavailable, "AutoSetup.openBrowserUnsupported"))
}

fn window_placement(window: &tauri::WebviewWindow, previous: Option<WindowPlacement>) -> Option<WindowPlacement> {
    let maximized = window.is_maximized().ok()?;
    if maximized {
        // Java keeps the normal bounds of a maximized window (`noneMax`).
        return previous.map(|p| WindowPlacement { maximized: true, ..p });
    }
    let pos = window.outer_position().ok()?;
    // GTK's native window size can include client-side decorations and shadows on Wayland.
    // Persist the WebKit allocation, which matches the content size accepted at startup.
    #[cfg(target_os = "linux")]
    let size = window
        .as_ref()
        .bounds()
        .ok()?
        .size
        .to_physical::<u32>(window.scale_factor().ok()?);
    #[cfg(not(target_os = "linux"))]
    let size = window.inner_size().ok()?;
    Some(WindowPlacement {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        maximized: false,
    })
}

/// Window close and application Quit share the same save barrier, the exit automatic save and
/// window persistence (`Lizzie.shutdown`: running saves first, then `saveAutoGame(1)`).
/// Capture placement before the window is destroyed, not in the final RunEvent::Exit.
fn prepare_exit(app: &AppHandle, shared: &Shared) -> bool {
    // Force exit destroys the window first (Java `System.exit`, no automatic save). It must not
    // enter a save barrier whose continuation closes that window, or the application would
    // remain headless.
    if app.get_webview_window("main").is_none() {
        return true;
    }
    let mut wb = shared.workbench.lock().unwrap();
    if wb.has_pending_saves() {
        shared.close_after_saves.store(true, Ordering::SeqCst);
        shared.log.info("exit waits for running saves");
        return false;
    }
    if wb.auto_save_on_exit() && !output::exit_saved(app, shared) {
        return false;
    }
    if let Some(frame) = app.get_webview_window(SUGGESTIONS_WINDOW) {
        shared.tables.lock().unwrap().capture_frame(&frame);
    }
    if let Some(hawkeye) = app.get_webview_window(HAWKEYE_WINDOW) {
        record_hawkeye(shared, &mut wb, &hawkeye);
    }
    if let Err(error) = independent::capture(app, &mut wb) { shared.log.warn(&error.message); }
    persist_ui(app, shared, &mut wb);
    true
}

/// `Config.persist`: the main window's placement, the 独立选点列表's last placement and the main
/// 选点列表's column widths. Java writes it on exit and whenever the frame closes
/// (`LizzieFrame.toggleBestMoves`).
fn persist_ui(app: &AppHandle, shared: &Shared, wb: &mut Workbench) {
    let placement = app
        .get_webview_window("main")
        .and_then(|w| window_placement(&w, wb.persisted().window));
    let (frame, main_list) = {
        let tables = shared.tables.lock().unwrap();
        (tables.frame_last.clone(), tables.main_list.clone())
    };
    if let Err(e) = wb.persist_ui(placement, frame, main_list) {
        shared.log.warn(&e.message);
    }
}

/// Records the open frame's bounds and writes them, as closing an AnalysisFrame does.
fn frame_closing(app: &AppHandle, shared: &Shared, frame: &tauri::WebviewWindow) {
    shared.tables.lock().unwrap().capture_frame(frame);
    persist_ui(app, shared, &mut shared.workbench.lock().unwrap());
}

/// The bounds an AnalysisFrame starts with in a layout (AnalysisFrame.java:260-302): its saved
/// bounds, else Java's default (-9, 550 / 480, 650 x 320); a saved 7-column position beyond the
/// primary screen moves to (0, 0). Also the column widths to lay out (empty: the defaults).
fn frame_bounds(app: &AppHandle, shared: &Shared, nine: bool) -> ((i32, i32, u32, u32), Vec<u32>) {
    let saved = {
        let tables = shared.tables.lock().unwrap();
        if nine {
            tables.frame_9.clone()
        } else {
            tables.frame_7.clone()
        }
    };
    let Some(p) = saved else {
        return ((-9, if nine { 550 } else { 480 }, 650, 320), Vec::new());
    };
    let (mut x, mut y) = (p.x, p.y);
    if !nine {
        if let Ok(Some(screen)) = app.primary_monitor() {
            let size = screen.size().to_logical::<i32>(screen.scale_factor());
            if x >= size.width || y >= size.height {
                (x, y) = (0, 0);
            }
        }
    }
    ((x, y, p.width, p.height), p.columns)
}

/// Opens the 独立选点列表 (`LizzieFrame.toggleBestMoves`, `AnalysisFrame` constructor) in the
/// layout for 9 columns (score data) or 7. With the frame open this is Java's rebuild (a box
/// toggled): it is closed and written, then placed again for the layout now. Returns the column
/// widths to lay out (empty: the defaults). Async: building a window inside a synchronous command
/// deadlocks on Windows.
#[tauri::command]
async fn open_suggestions_frame(
    app: AppHandle,
    state: tauri::State<'_, State>,
    nine: bool,
    on_top: bool,
    title: String,
) -> Result<Vec<u32>, ApiError> {
    if let Some(window) = app.get_webview_window(SUGGESTIONS_WINDOW) {
        frame_closing(&app, &state, &window);
        let ((x, y, width, height), columns) = frame_bounds(&app, &state, nine);
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        return Ok(columns);
    }
    let ((x, y, width, height), columns) = frame_bounds(&app, &state, nine);
    WebviewWindowBuilder::new(&app, SUGGESTIONS_WINDOW, WebviewUrl::App("suggestions.html".into()))
        .title(title)
        .inner_size(f64::from(width), f64::from(height))
        .position(f64::from(x), f64::from(y))
        .always_on_top(on_top)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| {
            ApiError::new(
                ErrorKind::Unavailable,
                format!("opening the candidates list failed: {e}"),
            )
        })?;
    Ok(columns)
}

/// Opens the secondary engine's 独立选点列表 next to the primary one (`AnalysisFrame(2)`): the
/// saved layout for its own column count, moved to the primary frame's right edge. Its bounds
/// are never written; Java only persists the primary frame.
#[tauri::command]
async fn open_suggestions_sub(
    app: AppHandle,
    state: tauri::State<'_, State>,
    nine: bool,
    on_top: bool,
    title: String,
) -> Result<Vec<u32>, ApiError> {
    let ((mut x, mut y, width, height), columns) = frame_bounds(&app, &state, nine);
    if let Some(primary) = app.get_webview_window(SUGGESTIONS_WINDOW) {
        let scale = primary.scale_factor().unwrap_or(1.0);
        if let (Ok(at), Ok(size)) = (primary.outer_position(), primary.outer_size()) {
            let (at, size) = (at.to_logical::<i32>(scale), size.to_logical::<i32>(scale));
            (x, y) = (at.x + size.width, at.y);
        }
    }
    if let Some(window) = app.get_webview_window(SUGGESTIONS_SUB_WINDOW) {
        let _ = window.set_size(tauri::LogicalSize::new(width, height));
        let _ = window.set_position(tauri::LogicalPosition::new(x, y));
        return Ok(columns);
    }
    WebviewWindowBuilder::new(&app, SUGGESTIONS_SUB_WINDOW, WebviewUrl::App("suggestions.html".into()))
        .title(title)
        .inner_size(f64::from(width), f64::from(height))
        .position(f64::from(x), f64::from(y))
        .always_on_top(on_top)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| {
            ApiError::new(
                ErrorKind::Unavailable,
                format!("opening the secondary candidates list failed: {e}"),
            )
        })?;
    Ok(columns)
}

/// Closes the secondary engine's 独立选点列表 (leaving 双引擎模式).
#[tauri::command]
fn close_suggestions_sub(app: AppHandle) {
    if let Some(window) = app.get_webview_window(SUGGESTIONS_SUB_WINDOW) {
        let _ = window.destroy();
    }
}

/// Closes the 独立选点列表 from the main window (U, the menu), writing its bounds; the secondary
/// engine's list closes with it (`toggleBestMoves`).
#[tauri::command]
fn close_suggestions_frame(app: AppHandle, state: tauri::State<'_, State>) {
    if let Some(window) = app.get_webview_window(SUGGESTIONS_WINDOW) {
        frame_closing(&app, &state, &window);
        let _ = window.destroy();
    }
    close_suggestions_sub(app);
}

/// The main 选点列表's column widths as laid out, `None` while its score column is hidden.
#[tauri::command]
fn set_main_list_columns(state: tauri::State<'_, State>, columns: Option<Vec<u32>>) {
    state.tables.lock().unwrap().main_list = columns;
}

/// The open 独立选点列表's column widths as laid out (7 or 9), relayed by the main window for the
/// primary list only: the secondary list's widths are not remembered.
#[tauri::command]
fn set_suggestions_columns(state: tauri::State<'_, State>, columns: Vec<u32>) {
    state.tables.lock().unwrap().frame_columns = columns;
}

/// The bounds a 超级鹰眼 window starts with (`MoveListFrame` constructor, 272-290): the saved
/// bounds, the secondary window to the right of them; a saved position beyond the primary screen
/// moves to (0, 0). Without saved bounds: (-9, 0), the secondary window at (737, 0), 856 x 565
/// (996 wide when the UI language is not Chinese).
fn hawkeye_bounds(app: &AppHandle, shared: &Shared, sub: bool, chinese: bool) -> (i32, i32, u32, u32) {
    let saved = shared.workbench.lock().unwrap().persisted().hawkeye.clone();
    let Some(p) = saved else {
        return (if sub { 737 } else { -9 }, 0, if chinese { 856 } else { 996 }, 565);
    };
    let (mut x, mut y) = (if sub { p.x + p.width as i32 } else { p.x }, p.y);
    if let Ok(Some(screen)) = app.primary_monitor() {
        let size = screen.size().to_logical::<i32>(screen.scale_factor());
        if x >= size.width - 50 || y >= size.height {
            (x, y) = (0, 0);
        }
    }
    (x, y, p.width, p.height)
}

/// Writes the primary 超级鹰眼's placement with the sort column and widths it last reported
/// (`Config.persist`, `badmoves-list-position`).
fn record_hawkeye(shared: &Shared, wb: &mut Workbench, window: &tauri::WebviewWindow) {
    let (sort, columns) = shared.tables.lock().unwrap().hawkeye.clone();
    let (Ok(scale), Ok(pos), Ok(size)) = (window.scale_factor(), window.outer_position(), window.inner_size()) else {
        return;
    };
    let (pos, size) = (pos.to_logical::<i32>(scale), size.to_logical::<u32>(scale));
    let placement = lizzie_core::settings::HawkeyePlacement {
        x: pos.x,
        y: pos.y,
        width: size.width,
        height: size.height,
        sort,
        columns,
    };
    if let Err(e) = wb.persist_hawkeye(placement) {
        shared.log.warn(&e.message);
    }
}

/// Opens a 超级鹰眼 window (`toggleBadMoves`), or retitles the open one; `sub` is 双引擎模式's
/// secondary engine window. Returns the saved sort column and 详细列表 widths. Async: building a
/// window inside a synchronous command deadlocks on Windows.
#[tauri::command]
async fn open_hawkeye(
    app: AppHandle,
    state: tauri::State<'_, State>,
    sub: bool,
    chinese: bool,
    on_top: bool,
    title: String,
) -> Result<(i32, Vec<u32>), ApiError> {
    let label = if sub { HAWKEYE_SUB_WINDOW } else { HAWKEYE_WINDOW };
    let layout = state.tables.lock().unwrap().hawkeye.clone();
    if let Some(window) = app.get_webview_window(label) {
        let _ = window.set_title(&title);
        let _ = window.set_always_on_top(on_top);
        return Ok(layout);
    }
    let (x, y, width, height) = hawkeye_bounds(&app, &state, sub, chinese);
    WebviewWindowBuilder::new(&app, label, WebviewUrl::App("hawkeye.html".into()))
        .title(title)
        .inner_size(f64::from(width), f64::from(height))
        .position(f64::from(x), f64::from(y))
        .always_on_top(on_top)
        .disable_drag_drop_handler()
        .build()
        .map_err(|e| ApiError::new(ErrorKind::Unavailable, format!("opening the hawk eye failed: {e}")))?;
    Ok(layout)
}

/// Opens 批量分析进度表 (`openAnalysisTable`), or brings the open one forward: a fixed 617 x 317
/// window centred on the main window (`setLocationRelativeTo(Lizzie.frame)`, not resizable).
/// Async: building a window inside a synchronous command deadlocks on Windows.
#[tauri::command]
async fn open_batch_table(app: AppHandle, title: String, on_top: bool) -> Result<(), ApiError> {
    if let Some(window) = app.get_webview_window(BATCH_WINDOW) {
        let _ = window.set_title(&title);
        let _ = window.show();
        let _ = window.set_focus();
        return Ok(());
    }
    let (width, height) = (617.0, 317.0);
    let mut builder = WebviewWindowBuilder::new(&app, BATCH_WINDOW, WebviewUrl::App("batch.html".into()))
        .title(title)
        .inner_size(width, height)
        .resizable(false)
        .always_on_top(on_top)
        .disable_drag_drop_handler();
    if let Some(main) = app.get_webview_window("main") {
        if let (Ok(scale), Ok(pos), Ok(size)) = (main.scale_factor(), main.outer_position(), main.outer_size()) {
            let (pos, size) = (pos.to_logical::<f64>(scale), size.to_logical::<f64>(scale));
            builder = builder.position(pos.x + (size.width - width) / 2.0, pos.y + (size.height - height) / 2.0);
        }
    }
    builder
        .build()
        .map_err(|e| ApiError::new(ErrorKind::Unavailable, format!("opening the batch table failed: {e}")))?;
    Ok(())
}

/// Closes both 超级鹰眼 windows from the main window (Y, the menus), writing the primary one's
/// placement.
#[tauri::command]
fn close_hawkeye(app: AppHandle, state: tauri::State<'_, State>) {
    if let Some(window) = app.get_webview_window(HAWKEYE_WINDOW) {
        record_hawkeye(&state, &mut state.workbench.lock().unwrap(), &window);
        let _ = window.destroy();
    }
    close_hawkeye_sub(app);
}

/// Closes the secondary engine's 超级鹰眼 (leaving 双引擎模式).
#[tauri::command]
fn close_hawkeye_sub(app: AppHandle) {
    if let Some(window) = app.get_webview_window(HAWKEYE_SUB_WINDOW) {
        let _ = window.destroy();
    }
}

/// The primary 超级鹰眼's sort column and 详细列表 widths, as it lays them out.
#[tauri::command]
fn set_hawkeye_layout(state: tauri::State<'_, State>, sort: i32, columns: Vec<u32>) {
    state.tables.lock().unwrap().hawkeye = (sort, columns);
}

/// The main 选点列表's saved column widths, for its first layout.
#[tauri::command]
fn main_list_columns(state: tauri::State<'_, State>) -> Option<Vec<u32>> {
    state.tables.lock().unwrap().main_list.clone()
}

/// Cocoa's predefined Quit calls terminate: without Tauri's cancellable ExitRequested.
/// Keep the default native menu, replacing only Quit with the shared exit request.
#[cfg(target_os = "macos")]
fn install_macos_quit(app: &AppHandle) -> Result<(), Box<dyn std::error::Error>> {
    use tauri::menu::{Menu, MenuItem, PredefinedMenuItem};
    let menu = Menu::default(app)?;
    let quit_text = PredefinedMenuItem::quit(app, None)?.text()?;
    for item in menu.items()? {
        let Some(submenu) = item.as_submenu() else { continue };
        for (index, item) in submenu.items()?.iter().enumerate() {
            let Some(predefined) = item.as_predefined_menuitem() else {
                continue;
            };
            if predefined.text()? == quit_text {
                submenu.remove_at(index)?;
                submenu.insert(&MenuItem::with_id(app, "quit", &quit_text, true, Some("Cmd+Q"))?, index)?;
                app.set_menu(menu)?;
                app.on_menu_event(|app, event| {
                    if event.id().as_ref() == "quit" {
                        app.exit(0);
                    }
                });
                return Ok(());
            }
        }
    }
    Err(std::io::Error::other("Default macOS menu has no Quit item").into())
}

pub fn run() {
    let context = tauri::generate_context!();
    let logs = startup::logs_dir(&context.config().identifier);
    if let Some(dir) = &logs {
        logging::install_panic_hook(dir.clone());
    }
    // The installer brings WebView2 along; the portable build relies on the system copy.
    #[cfg(windows)]
    if let Err(e) = tauri::webview_version() {
        startup::report(logs.as_deref(), &startup::Failure::MissingWebView(e.to_string()));
        std::process::exit(1);
    }
    let app = tauri::Builder::default()
        .plugin(tauri_plugin_dialog::init())
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_clipboard_manager::init())
        .setup(|app| {
            #[cfg(target_os = "macos")]
            install_macos_quit(app.handle())?;
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
            let tables = TableLayout::new(workbench.persisted());
            let shared: State = Arc::new(Shared {
                workbench: Mutex::new(workbench),
                log,
                refresh: Mutex::new((false, false, false)),
                refresh_cv: Condvar::new(),
                processes,
                saves: Mutex::new(spawn_save_lane(app.handle().clone())),
                close_after_saves: AtomicBool::new(false),
                exit_save: Mutex::new(output::ExitSave::Idle),
                views: Mutex::new(ViewSubscription::default()),
                tables: Mutex::new(tables),
            });
            let _ = cell.set(shared.clone());
            app.manage(shared.clone());
            run_refresher(app.handle().clone(), shared.clone());
            shared.workbench.lock().unwrap().start_engines();
            publish(app.handle(), &shared);
            resources::startup(app.handle().clone(), shared.clone());
            tuning::startup(app.handle().clone(), shared.clone());

            let window = app.get_webview_window("main").expect("main window");
            if let Some(p) = placement {
                let _ = window.set_size(PhysicalSize::new(p.width, p.height));
                let _ = window.set_position(PhysicalPosition::new(p.x, p.y));
                if p.maximized {
                    let _ = window.maximize();
                }
            }

            // Command line: one argument other than "read" opens that record without touching
            // the recent list (Lizzie.java:1049-1052); without one, 启动时自动恢复 resumes the
            // automatic slot (`resumeFile`).
            let args: Vec<String> = std::env::args().skip(1).filter(|a| !a.starts_with("--")).collect();
            if let [path] = args.as_slice() {
                if path != "read" {
                    start_open(app.handle(), &shared, path.clone(), OpenSource::CommandLine);
                }
            } else {
                let resume = shared.workbench.lock().unwrap().begin_resume();
                if let Some((ticket, path)) = resume {
                    shared.log.info("resuming the automatic slot");
                    read_record(app.handle(), &shared, ticket, path);
                }
            }

            #[cfg(feature = "acceptance")]
            acceptance::start(app.handle().clone(), shared.clone());
            Ok(())
        })
        .on_window_event(|window, event| match event {
            WindowEvent::CloseRequested { .. } if independent::board(window.label()).is_some() => {
                let app = window.app_handle();
                let shared = app.state::<State>();
                let board = independent::board(window.label()).unwrap();
                if let Some(frame) = app.get_webview_window(window.label()) {
                    independent::closing(&shared, &frame, board);
                }
                publish(app, &shared);
            }
            // Either 独立选点列表's own close box (AnalysisFrame windowClosing → toggleBestMoves):
            // both lists close and the primary one's bounds are written.
            WindowEvent::CloseRequested { .. }
                if matches!(window.label(), SUGGESTIONS_WINDOW | SUGGESTIONS_SUB_WINDOW) =>
            {
                let app = window.app_handle();
                if let Some(frame) = app.get_webview_window(SUGGESTIONS_WINDOW) {
                    frame_closing(app, &app.state::<State>(), &frame);
                }
                for label in [SUGGESTIONS_WINDOW, SUGGESTIONS_SUB_WINDOW] {
                    if label != window.label() {
                        if let Some(other) = app.get_webview_window(label) {
                            let _ = other.destroy();
                        }
                    }
                }
                let _ = app.emit_to("main", SUGGESTIONS_CLOSED_EVENT, ());
            }
            // Either 超级鹰眼's own close box (MoveListFrame closing → toggleBadMoves): both close,
            // the primary one's placement is written and the main window turns it off.
            WindowEvent::CloseRequested { .. } if matches!(window.label(), HAWKEYE_WINDOW | HAWKEYE_SUB_WINDOW) => {
                let app = window.app_handle();
                let shared = app.state::<State>();
                if let Some(primary) = app.get_webview_window(HAWKEYE_WINDOW) {
                    record_hawkeye(&shared, &mut shared.workbench.lock().unwrap(), &primary);
                }
                for label in [HAWKEYE_WINDOW, HAWKEYE_SUB_WINDOW] {
                    if label != window.label() {
                        if let Some(other) = app.get_webview_window(label) {
                            let _ = other.destroy();
                        }
                    }
                }
                let _ = app.emit_to("main", HAWKEYE_CLOSED_EVENT, ());
            }
            // 批量分析进度表's own close box closes only it (AnalysisTable's JFrame does not exit).
            WindowEvent::CloseRequested { .. } if window.label() == BATCH_WINDOW => {}
            WindowEvent::CloseRequested { api, .. } => {
                let app = window.app_handle();
                let shared = app.state::<State>();
                if !prepare_exit(app, &shared) {
                    api.prevent_close();
                } else {
                    // Java exits with the main window; the tool windows must not keep the
                    // application running.
                    for label in [SUGGESTIONS_WINDOW, SUGGESTIONS_SUB_WINDOW, HAWKEYE_WINDOW, HAWKEYE_SUB_WINDOW, BATCH_WINDOW, independent::MAIN, independent::SUB] {
                        if let Some(frame) = app.get_webview_window(label) {
                            let _ = frame.destroy();
                        }
                    }
                }
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
                    // Several files start 批量分析 (`openOrdinaryBatchAnalysis(files, true)`).
                    _ => {
                        let paths = paths.iter().map(|p| p.to_string_lossy().into_owned()).collect();
                        let mut wb = shared.workbench.lock().unwrap();
                        if let Err(key) = wb.open_batch(BatchMode::Ordinary, paths, OpenSource::Drop) {
                            wb.refuse_batch(key);
                        }
                        drop(wb);
                        publish(app, &shared);
                    }
                }
            }
            _ => {}
        })
        .invoke_handler(handlers())
        .build(context);
    match app {
        Ok(app) => app.run(|app, event| {
            if let RunEvent::ExitRequested { api, .. } = event {
                let shared = app.state::<State>();
                if !prepare_exit(app, &shared) {
                    api.prevent_exit();
                }
            } else if let RunEvent::Exit = event {
                // Every exit path (window close, 退出, 强制退出) ends the engines and waits for
                // their processes, so none is left behind.
                let shared = app.state::<State>().inner().clone();
                shared.workbench.lock().unwrap().end_engines();
                shared.workbench.lock().unwrap().stop_benchmark();
                shared.workbench.lock().unwrap().cancel_resource_tasks();
                resources::shutdown();
                tuning::shutdown();
                shared.processes.wait(Duration::from_secs(4));
                let logs = shared.workbench.lock().unwrap().take_logs();
                write_logs(&shared, logs);
                shared.log.info("engines ended");
                shared.log.info("exit");
            }
        }),
        Err(e) => {
            startup::report(logs.as_deref(), &startup::Failure::Other(e.to_string()));
            std::process::exit(1);
        }
    }
}

#[cfg(not(feature = "acceptance"))]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dispatch_command,
        workbench_snapshot,
        search_functions,
        subscribe_workbench,
        unsubscribe_workbench,
        logs_directory,
        engine_console,
        open_katago_releases,
        output::recovery_thumbnail,
        open_suggestions_frame,
        close_suggestions_frame,
        open_suggestions_sub,
        close_suggestions_sub,
        set_main_list_columns,
        set_suggestions_columns,
        main_list_columns,
        open_hawkeye,
        close_hawkeye,
        close_hawkeye_sub,
        set_hawkeye_layout,
        independent::open_independent_board,
        independent::close_independent_board_window,
        open_batch_table
    ]
}

#[cfg(feature = "acceptance")]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dispatch_command,
        workbench_snapshot,
        search_functions,
        subscribe_workbench,
        unsubscribe_workbench,
        logs_directory,
        engine_console,
        open_katago_releases,
        output::recovery_thumbnail,
        open_suggestions_frame,
        close_suggestions_frame,
        open_suggestions_sub,
        close_suggestions_sub,
        set_main_list_columns,
        set_suggestions_columns,
        main_list_columns,
        open_hawkeye,
        close_hawkeye,
        close_hawkeye_sub,
        set_hawkeye_layout,
        independent::open_independent_board,
        independent::close_independent_board_window,
        open_batch_table,
        acceptance::acceptance_ui_report
    ]
}
