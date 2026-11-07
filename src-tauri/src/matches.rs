//! Match files use the same core commands and serial save lane as the workbench.
use crate::{publish, State};
use lizzie_core::api::{ApiError, CommandResult, ErrorKind, MatchCommand, MatchOpeningFile};
use tauri::AppHandle;
use tauri_plugin_dialog::DialogExt;

#[tauri::command]
pub async fn open_match_openings(
    app: AppHandle,
    state: tauri::State<'_, State>,
    paths: Vec<String>,
) -> Result<CommandResult, ApiError> {
    let shared = state.inner().clone();
    let paths = if paths.is_empty() {
        let dialog = app.dialog().clone();
        let last_folder = shared.workbench.lock().unwrap().persisted().last_folder.clone();
        let start = crate::chooser_start(&app, last_folder);
        tauri::async_runtime::spawn_blocking(move || {
            let mut builder = dialog.file().add_filter("SGF (*.sgf)", &["sgf", "SGF"]);
            if let Some(start) = start {
                builder = builder.set_directory(start);
            }
            builder.blocking_pick_files()
        })
        .await
        .map_err(|e| ApiError::new(ErrorKind::Io, e.to_string()))?
        .unwrap_or_default()
        .into_iter()
        .filter_map(|p| p.into_path().ok())
        .map(|p| p.to_string_lossy().into_owned())
        .collect()
    } else {
        paths
    };
    if paths.is_empty() {
        return Ok(CommandResult::Unchanged);
    }
    let files = tauri::async_runtime::spawn_blocking(move || {
        paths
            .into_iter()
            .map(|path| {
                let bytes = std::fs::read(&path).map_err(|e| ApiError::new(ErrorKind::Io, format!("{path}: {e}")))?;
                Ok(MatchOpeningFile { path, bytes })
            })
            .collect::<Result<Vec<_>, ApiError>>()
    })
    .await
    .map_err(|e| ApiError::new(ErrorKind::Io, e.to_string()))??;
    let result = shared
        .workbench
        .lock()
        .unwrap()
        .engine_match(MatchCommand::ReplaceOpenings { files })?;
    publish(&app, &shared);
    Ok(result)
}

#[tauri::command]
pub async fn write_match_winrate(
    app: AppHandle,
    state: tauri::State<'_, State>,
    game: u64,
    bytes: Vec<u8>,
) -> Result<CommandResult, ApiError> {
    let shared = state.inner().clone();
    let Some(path) = shared.workbench.lock().unwrap().match_winrate_path(game) else {
        return Ok(CommandResult::Unchanged);
    };
    let error = tauri::async_runtime::spawn_blocking(move || {
        if !bytes.starts_with(b"\x89PNG\r\n\x1a\n") {
            return Err("invalid PNG".to_string());
        }
        if let Some(parent) = path.parent() {
            std::fs::create_dir_all(parent).map_err(|e| e.to_string())?;
        }
        lizzie_core::formats::write_file_atomic(&path, &bytes).map_err(|e| e.to_string())
    })
    .await
    .map_err(|e| ApiError::new(ErrorKind::Io, e.to_string()))?
    .err();
    let result = shared
        .workbench
        .lock()
        .unwrap()
        .engine_match(MatchCommand::WinrateImageDone { game, error })?;
    publish(&app, &shared);
    Ok(result)
}
