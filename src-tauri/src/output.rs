//! 文件 > 更多保存, the clipboard copies and 存档与读档 in the platform adapter: system choosers,
//! picture writing off the UI thread, the clipboard, slot loads and the exit automatic save.

use std::path::Path;
use std::time::Duration;

use lizzie_core::api::{ApiError, CommandResult, CommentLabels, ErrorKind, ExportKind, RecoveryCommand, SlotRef};
use lizzie_core::formats::{self, picture};
use lizzie_core::workbench::RecoveryStep;
use tauri::{AppHandle, Emitter, Manager};
use tauri_plugin_clipboard_manager::ClipboardExt;
use tauri_plugin_dialog::DialogExt;

use crate::{application, chooser_start, logging, publish, queue_write, read_record, Shared, State};

/// The window is closing with 自动存档 on: the UI answers with `Recovery ExitAutoSave` and the
/// main-board picture.
pub const EXIT_AUTOSAVE_EVENT: &str = "exit-autosave-request";
/// Without an answer the automatic slot is written without a picture after this long.
const EXIT_AUTOSAVE_WAIT: Duration = Duration::from_secs(2);

/// The exit automatic save of the closing window.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ExitSave {
    Idle,
    /// Asked the UI for the picture.
    Requested,
    /// One writer (the UI answer or the fallback) took it and is writing; the exit still waits.
    Writing,
    /// Written, or the attempt ended; the exit may proceed.
    Done,
}

fn unavailable(e: impl std::fmt::Display) -> ApiError {
    ApiError::new(ErrorKind::Unavailable, e.to_string())
}

// ------------------------------------------------------------------ record exports

/// 保存纯净棋谱 / 带评论 / 纯净分支: the save chooser like "另存为" (last folder, `*.sgf`, the
/// suggested name), then the export (`SgfSaveCoordinator.chooseAndSave(mode)`).
pub async fn export_record(
    app: &AppHandle,
    shared: &State,
    kind: ExportKind,
    suggested_name: String,
    labels: CommentLabels,
) -> Result<CommandResult, ApiError> {
    let last_folder = shared.workbench.lock().unwrap().persisted().last_folder.clone();
    let start_dir = chooser_start(app, last_folder);
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
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
    .map_err(unavailable)?;
    let Some(selected) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(CommandResult::Unchanged);
    };
    let selected = selected.to_string_lossy().into_owned();
    let target = formats::save_target(&selected);
    if target != selected && Path::new(&target).exists() {
        return Ok(CommandResult::NeedsExportOverwriteConfirmation { kind, path: target });
    }
    Ok(start_export(app, shared, kind, &target, &labels))
}

/// Captures export `kind` and hands it to the serial writer.
pub fn start_export(
    app: &AppHandle,
    shared: &State,
    kind: ExportKind,
    path: &str,
    labels: &CommentLabels,
) -> CommandResult {
    let job = shared.workbench.lock().unwrap().begin_export(kind, labels, path);
    queue_write(app, shared, job)
}

/// 复制棋谱到剪贴板.
pub fn copy_record(app: &AppHandle, shared: &State) -> Result<CommandResult, ApiError> {
    let text = shared.workbench.lock().unwrap().record_text(&application(app));
    app.clipboard().write_text(text).map_err(unavailable)?;
    Ok(CommandResult::Done)
}

// ------------------------------------------------------------------ pictures

/// 保存主棋盘截图 / 小棋盘截图 / 胜率图: the picture chooser (Java's four filters, PNG first; the
/// last picture folder), then the picture in the format of the chosen name.
pub async fn export_image(app: &AppHandle, shared: &State, png: String) -> Result<CommandResult, ApiError> {
    let png = picture::png_from_base64(&png).map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
    let folder = shared.workbench.lock().unwrap().picture_folder();
    let start_dir = chooser_start(app, folder);
    let dialog = app.dialog().clone();
    let picked = tauri::async_runtime::spawn_blocking(move || {
        let mut builder = dialog.file();
        for (name, extensions) in picture::FILTERS {
            builder = builder.add_filter(*name, extensions);
        }
        if let Some(dir) = start_dir {
            builder = builder.set_directory(dir);
        }
        builder.blocking_save_file()
    })
    .await
    .map_err(unavailable)?;
    let Some(selected) = picked.and_then(|p| p.into_path().ok()) else {
        return Ok(CommandResult::Unchanged);
    };
    let selected = selected.to_string_lossy().into_owned();
    shared.workbench.lock().unwrap().remember_picture_folder(&selected);
    let (target, _) = picture::picture_target(&selected);
    if target != selected && Path::new(&target).exists() {
        return Ok(CommandResult::NeedsImageOverwriteConfirmation { path: target });
    }
    Ok(write_picture(shared, target, png).await)
}

/// Writes a picture to a known path (after the replace prompt), exactly as named.
pub async fn export_image_to(shared: &State, path: String, png: String) -> Result<CommandResult, ApiError> {
    let png = picture::png_from_base64(&png).map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
    Ok(write_picture(shared, path, png).await)
}

async fn write_picture(shared: &State, path: String, png: Vec<u8>) -> CommandResult {
    let target = path.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        let (_, format) = picture::picture_target(&target);
        let bytes = picture::encode(&png, format)?;
        formats::write_file_atomic(Path::new(&target), &bytes).map_err(|e| e.to_string())
    })
    .await
    .unwrap_or_else(|e| Err(e.to_string()));
    match result {
        Ok(()) => {
            shared
                .log
                .info(&format!("picture written ({})", logging::file_label(&path)));
            CommandResult::Exported { path }
        }
        Err(detail) => {
            shared
                .log
                .warn(&format!("picture failed ({}): {detail}", logging::file_label(&path)));
            CommandResult::ExportFailed { path, detail }
        }
    }
}

/// 复制主棋盘到剪贴板 / 复制小棋盘到剪贴板.
pub fn copy_image(app: &AppHandle, shared: &State, png: &str) -> Result<CommandResult, ApiError> {
    let png = picture::png_from_base64(png).map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
    let (rgba, width, height) = picture::rgba(&png).map_err(|e| ApiError::new(ErrorKind::InvalidArgument, e))?;
    app.clipboard()
        .write_image(&tauri::image::Image::new_owned(rgba, width, height))
        .map_err(|e| {
            shared.log.warn(&format!("clipboard picture failed: {e}"));
            unavailable(e)
        })?;
    Ok(CommandResult::Done)
}

// ------------------------------------------------------------------ 存档与读档

pub fn recovery(app: &AppHandle, shared: &State, op: RecoveryCommand) -> Result<CommandResult, ApiError> {
    let exit = matches!(op, RecoveryCommand::ExitAutoSave { .. });
    // Automatic saves fail quietly into the log, as Java's `saveAutoGame` does; a dialog every
    // minute would block the user. The panel's own operations report their failure.
    let automatic = exit || matches!(op, RecoveryCommand::AutoSave { .. });
    let claimed = exit && claim_exit_save(shared);
    if exit && !claimed && *shared.exit_save.lock().unwrap() != ExitSave::Idle {
        // The other writer has it. Without a waiting exit (Idle) an automation call writes the
        // automatic slot without closing anything.
        return Ok(CommandResult::Unchanged);
    }
    let step = shared.workbench.lock().unwrap().recovery_command(op, &application(app));
    if claimed {
        finish_exit_save(app, shared, step.as_ref().err().map(|e| e.message.as_str()));
    }
    let step = match step {
        Ok(s) => s,
        Err(e) => {
            shared.log.warn(&format!("save and load: {}", e.message));
            return if automatic {
                Ok(CommandResult::Unchanged)
            } else {
                Err(e)
            };
        }
    };
    match step {
        RecoveryStep::Load { ticket, path } => {
            read_record(app, shared, ticket, path);
            Ok(CommandResult::OpenStarted { ticket })
        }
        RecoveryStep::Changed(changed) => {
            if changed {
                publish(app, shared);
            }
            Ok(if changed {
                CommandResult::Done
            } else {
                CommandResult::Unchanged
            })
        }
    }
}

/// The picture of a slot (empty when it has none), as raw bytes.
#[tauri::command]
pub fn recovery_thumbnail(state: tauri::State<'_, State>, slot: SlotRef) -> tauri::ipc::Response {
    let bytes = state.workbench.lock().unwrap().slot_thumbnail(slot).unwrap_or_default();
    tauri::ipc::Response::new(bytes)
}

/// Takes the exit save over from the waiting request: only one writer (the UI answer or the
/// fallback) writes it, and the exit keeps waiting until [`finish_exit_save`].
fn claim_exit_save(shared: &Shared) -> bool {
    let mut state = shared.exit_save.lock().unwrap();
    if *state == ExitSave::Requested {
        *state = ExitSave::Writing;
        true
    } else {
        false
    }
}

/// The claimed exit save ended (`failure` when it could not be written): the exit proceeds.
fn finish_exit_save(app: &AppHandle, shared: &Shared, failure: Option<&str>) {
    match failure {
        None => shared.log.info("exit automatic save written"),
        Some(e) => shared.log.warn(&format!("exit automatic save failed: {e}")),
    }
    *shared.exit_save.lock().unwrap() = ExitSave::Done;
    close_main(app);
}

fn close_main(app: &AppHandle) {
    if let Some(window) = app.get_webview_window("main") {
        let _ = window.close();
    }
}

/// Whether the exit automatic save is written; the first call asks the UI for the main-board
/// picture and starts the fallback, and the exit waits.
pub fn exit_saved(app: &AppHandle, shared: &Shared) -> bool {
    let mut state = shared.exit_save.lock().unwrap();
    match *state {
        ExitSave::Done => true,
        ExitSave::Requested | ExitSave::Writing => false,
        ExitSave::Idle => {
            *state = ExitSave::Requested;
            drop(state);
            let _ = app.emit(EXIT_AUTOSAVE_EVENT, ());
            let app = app.clone();
            std::thread::spawn(move || {
                std::thread::sleep(EXIT_AUTOSAVE_WAIT);
                let shared = app.state::<State>().inner().clone();
                if claim_exit_save(&shared) {
                    shared
                        .log
                        .warn("exit automatic save without the board picture (no answer)");
                    let result = shared
                        .workbench
                        .lock()
                        .unwrap()
                        .recovery_command(RecoveryCommand::ExitAutoSave { thumbnail: None }, &application(&app));
                    finish_exit_save(&app, &shared, result.as_ref().err().map(|e| e.message.as_str()));
                }
            });
            false
        }
    }
}
