//! Read-only image transport and system font discovery. Work is done off the UI thread and
//! outside the workbench lock. IPC returns decoded image bytes as data URLs: no blanket asset
//! protocol scope is needed for either user themes or the rest of the filesystem.
use lizzie_core::appearance::{self, AppearanceCatalog, AppearanceSettings, ThemeImages, ThemeSettings};
use crate::{State, ApiError, ErrorKind};

fn error(e: impl std::fmt::Display) -> ApiError { ApiError::new(ErrorKind::Io,format!("appearance: {e}")) }

#[tauri::command]
pub async fn appearance_catalog(state:tauri::State<'_,State>) -> Result<AppearanceCatalog,ApiError> {
    let (storage,defaults) = { let wb=state.workbench.lock().unwrap(); (wb.storage().clone(),wb.config().appearance_defaults.clone()) };
    tauri::async_runtime::spawn_blocking(move || {
        // ConfigDialog2.java:4513-4520: GraphicsEnvironment.getAvailableFontFamilyNames.
        let mut db=fontdb::Database::new(); db.load_system_fonts();
        let mut fonts:Vec<String>=db.faces().flat_map(|face|face.families.iter().map(|(name,_)|name.clone())).collect();
        fonts.sort_by_key(|s|s.to_lowercase()); fonts.dedup();
        fonts.retain(|s|s!="Lizzie Default" && s!="Dialog.plain"); fonts.insert(0,"Dialog.plain".into()); fonts.insert(0,"Lizzie Default".into());
        AppearanceCatalog {themes:appearance::catalog(&storage,&defaults),fonts}
    }).await.map_err(error)
}
#[tauri::command]
pub async fn appearance_images(state:tauri::State<'_,State>,settings:AppearanceSettings,theme:ThemeSettings) -> Result<ThemeImages,ApiError> {
    let storage=state.workbench.lock().unwrap().storage().clone();
    tauri::async_runtime::spawn_blocking(move ||appearance::images(&storage,&settings,&theme)).await.map_err(error)
}

#[tauri::command]
pub async fn appearance_choose_image(app:tauri::AppHandle) -> Result<Option<String>,ApiError> {
    use tauri_plugin_dialog::DialogExt;
    let dialog=app.dialog().clone();
    // Menu.java:7049-7058: FILES_ONLY, with cancellation leaving settings untouched.
    tauri::async_runtime::spawn_blocking(move || {
        dialog.file().blocking_pick_file().and_then(|file|file.into_path().ok()).map(|path|path.to_string_lossy().into_owned())
    }).await.map_err(error)
}

#[tauri::command]
pub async fn strength_choose_model(app:tauri::AppHandle,title:String) -> Result<Option<String>,ApiError> {
    use tauri_plugin_dialog::DialogExt;
    let dialog=app.dialog().clone();
    tauri::async_runtime::spawn_blocking(move || {
        dialog.file().set_title(title).add_filter("XGBoost strength model (*.json, *.zip)",&["json","zip"])
            .blocking_pick_file().and_then(|file|file.into_path().ok()).map(|path|path.to_string_lossy().into_owned())
    }).await.map_err(error)
}
