//! Tauri platform adapter. Owns windows, native dialogs, the clipboard, threads and event
//! delivery; every state change goes through the core [`Workbench`].

#[cfg(feature = "acceptance")]
mod acceptance;
mod diagnostics;
mod logging;

use std::path::PathBuf;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{ApiError, Command, CommandResult, ErrorKind, LogsStatus, OpenSource, WorkbenchView};
use lizzie_core::formats;
use lizzie_core::settings::{Storage, WindowPlacement};
use lizzie_core::workbench::{OpenOrigin, PasteDecision, Workbench};
use tauri::{AppHandle, Emitter, Manager, PhysicalPosition, PhysicalSize, WindowEvent};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;
use tauri_plugin_opener::OpenerExt;

pub const VIEW_EVENT: &str = "workbench-view";
/// A Java entry whose capability is not implemented yet was used; payload names the capability.
pub const UNAVAILABLE_EVENT: &str = "capability-unavailable";

pub struct Shared {
    pub workbench: Mutex<Workbench>,
    pub log: logging::Log,
}

pub type State = Arc<Shared>;

/// Publishes the next view. Built and emitted under the lock so views arrive in `seq` order.
pub fn publish(app: &AppHandle, shared: &Shared) {
    let wb = shared.workbench.lock().unwrap();
    let mut wb = wb;
    let view = wb.view(false);
    let _ = app.emit(VIEW_EVENT, &view);
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
            Ok(CommandResult::Done)
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
    }
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
            let workbench = Workbench::new(storage, locale.as_deref());
            let placement = workbench.persisted().window;
            let shared: State = Arc::new(Shared {
                workbench: Mutex::new(workbench),
                log,
            });
            app.manage(shared.clone());

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
            WindowEvent::CloseRequested { .. } => {
                let Some(webview) = window.app_handle().get_webview_window(window.label()) else {
                    return;
                };
                let shared = window.app_handle().state::<State>().inner().clone();
                let mut wb = shared.workbench.lock().unwrap();
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
        .run(tauri::generate_context!())
        .expect("error while running LizzieYzy Tauri");
}

#[cfg(not(feature = "acceptance"))]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![dispatch_command, workbench_snapshot, logs_directory]
}

#[cfg(feature = "acceptance")]
fn handlers() -> impl Fn(tauri::ipc::Invoke) -> bool + Send + Sync + 'static {
    tauri::generate_handler![
        dispatch_command,
        workbench_snapshot,
        logs_directory,
        acceptance::acceptance_ui_report
    ]
}
