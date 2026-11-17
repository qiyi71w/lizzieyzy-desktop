//! Explicit, independently consented Java cloud credentials. Source stores are read-only.
use std::sync::Arc;
use lizzie_core::api::{ApiError, ErrorKind, ImportReason, ImportReportView, ImportStatus};
use lizzie_core::cloud::CloudSettings;
use lizzie_core::java_import::{self, CloudImportCredentials, JavaSource};
use tauri::{AppHandle, Manager};
use crate::cloud::{Adapter, CloudImportSettlement, CloudImportStage};

pub struct Staged {
    pub stage: CloudImportStage,
    token_requested: bool,
    password_requested: bool,
    token_found: bool,
    password_found: bool,
}

impl Staged {
    pub fn authenticated(&self) -> bool { self.token_found }
}

pub fn stage(app: &AppHandle, plan: Option<(JavaSource, CloudSettings, CloudImportCredentials)>,
    token: bool, password: bool) -> Result<Option<Staged>, ApiError> {
    let Some((source, mut settings, offered)) = plan else { return Ok(None) };
    let store = crate::secrets::Store::new(app.config().identifier.clone(),
        &app.path().app_config_dir().map_err(|_| ApiError::new(ErrorKind::Io, "cloud credential directory"))?);
    let read = |password| {
        store.read_java_cloud_secret(&source.config_dir, &offered.identifier, password)
            .ok().flatten().filter(|s| !s.is_empty())
            .or_else(|| java_import::read_java_cloud_secret(&source.config_dir, password))
    };
    let token_requested = token && offered.token;
    let password_requested = password && offered.password;
    let token = token_requested.then(|| read(false)).flatten();
    let password = password_requested.then(|| read(true)).flatten();
    let token_found = token.is_some();
    let password_found = password.is_some();
    settings.remember_token = token_found;
    settings.remember_password = password_found;
    let stage = app.state::<Arc<Adapter>>().stage_import(&settings, token, password)
        .map_err(|e| ApiError::new(ErrorKind::Unavailable, e.to_string()))?;
    Ok(Some(Staged { stage, token_requested, password_requested, token_found, password_found }))
}

pub fn settle(adapter: &Adapter, staged: Staged, keep: bool, report: Option<&mut ImportReportView>) -> Result<(), ApiError> {
    let CloudImportSettlement { reply, rollback_failed } = adapter.settle_import(staged.stage, keep);
    if let Some(report) = report.filter(|_| keep) {
        for (key, requested, found, stored) in [
            ("credential:cloud-token", staged.token_requested, staged.token_found, reply.state.token_stored_securely),
            ("credential:cloud-password", staged.password_requested, staged.password_found, reply.state.password_stored_securely),
        ] {
            if !requested { continue; }
            if let Some(item) = report.items.iter_mut().find(|i| i.key == key) {
                item.status = if found { ImportStatus::Imported } else { ImportStatus::Incompatible };
                item.reason = if !found { Some(ImportReason::Unreadable) } else if !stored { Some(ImportReason::SessionOnly) } else { None };
                item.detail = Some(if !found { "credential unavailable" } else if stored { "system credential store" } else { "session only; credential store unavailable" }.into());
            }
        }
    }
    if rollback_failed {
        return Err(ApiError::new(ErrorKind::Io, "Cloud credential rollback could not confirm every native-store value."));
    }
    Ok(())
}
