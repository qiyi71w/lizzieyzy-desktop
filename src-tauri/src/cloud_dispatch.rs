//! Desktop cloud operations: no network or credential-store IO under the workbench lock.
use std::collections::HashMap;
use std::sync::{atomic::{AtomicBool, Ordering}, Arc, Mutex};
use lizzie_core::api::{ApiError, CommandResult, ErrorKind};
use lizzie_core::cloud::{CloudCommand, CloudFailure, CloudSettings, Provider};
use tauri::{AppHandle, Manager};
use tauri_plugin_dialog::DialogExt;
use crate::{cloud::Adapter, State};

#[derive(Default)]
pub struct Requests(Mutex<HashMap<u64, Vec<Arc<AtomicBool>>>>);
impl Requests {
    pub(crate) fn register(&self, owner: u64) -> Arc<AtomicBool> {
        let cancel = Arc::new(AtomicBool::new(false));
        self.0.lock().unwrap().entry(owner).or_default().push(cancel.clone());
        cancel
    }
    pub(crate) fn cancel(&self, owner: u64, adapter: &Adapter) {
        let mut requests = self.0.lock().unwrap();
        if let Some(requests) = requests.remove(&owner) {
            for request in requests { adapter.cancel_scoped(&request); }
        }
    }
    fn finish(&self, owner: u64, cancel: &Arc<AtomicBool>, adapter: &Adapter) {
        let mut requests = self.0.lock().unwrap();
        adapter.cancel_scoped(cancel);
        if let Some(active) = requests.get_mut(&owner) {
            active.retain(|request| !Arc::ptr_eq(request,cancel));
            if active.is_empty() { requests.remove(&owner); }
        }
    }
}
struct OwnedRequest { app: AppHandle, owner: u64, cancel: Arc<AtomicBool> }
impl Drop for OwnedRequest {
    fn drop(&mut self) {
        self.app.state::<Requests>().finish(self.owner,&self.cancel,&self.app.state::<Arc<Adapter>>());
    }
}
pub fn cancel_request(app: &AppHandle, owner: u64) {
    app.state::<Requests>().cancel(owner,&app.state::<Arc<Adapter>>());
}
pub async fn account_scoped(app: AppHandle, shared: State, op: CloudCommand, owner: u64) -> Result<CommandResult, ApiError> {
    let request = OwnedRequest { cancel: app.state::<Requests>().register(owner), app: app.clone(), owner };
    account_inner(app,shared,op,request.cancel.clone(),true).await
}

pub async fn account(app: AppHandle, shared: State, op: CloudCommand) -> Result<CommandResult, ApiError> {
    account_inner(app,shared,op,Arc::new(AtomicBool::new(false)),false).await
}
async fn account_inner(app: AppHandle, shared: State, op: CloudCommand, cancel: Arc<AtomicBool>, scoped: bool) -> Result<CommandResult, ApiError> {
    let adapter = app.state::<Arc<Adapter>>().inner().clone();
    let (settings, network) = {
        let wb = shared.workbench.lock().unwrap();
        (wb.config().cloud.clone(), wb.config().network.clone())
    };
    let catalog_args_if_unchanged = matches!(&op, CloudCommand::RefreshCatalog { .. }).then(|| settings.args.clone());
    let uncertain_on_cancel = matches!(&op, CloudCommand::PasswordLogin {..} | CloudCommand::CodeLogin {..} | CloudCommand::ResetPassword {..} | CloudCommand::SendCode {..} | CloudCommand::CreateOrder {..});
    let worker = adapter.clone();
    let worker_cancel = cancel.clone();
    let result = tauri::async_runtime::spawn_blocking(move || worker.dispatch_scoped(&settings, &network, op, worker_cancel))
        .await.map_err(|_| ApiError::new(ErrorKind::Unavailable, "RemoteCompute.error.requestFailed"))?;
    // The same registry lock protects cancellation marking and settings admission. Once an
    // owner is marked closed, no successful reply from that owner can write settings.
    let registry = scoped.then(|| app.state::<Requests>());
    let admission = registry.as_ref().map(|requests| requests.0.lock().unwrap());
    if cancel.load(Ordering::Acquire) {
        return Ok(CommandResult::CloudError { failure: CloudFailure { uncertain: uncertain_on_cancel, ..CloudFailure::cancelled() } });
    }
    match result {
        Ok(mut reply) => {
            let mut wb = shared.workbench.lock().unwrap();
            if !adapter.is_current(reply.state.generation) {
                return Ok(CommandResult::CloudError { failure: CloudFailure::cancelled() });
            }
            wb.cloud_account_settings(&reply.settings, catalog_args_if_unchanged.as_deref())?;
            wb.set_cloud_authenticated(reply.state.authenticated);
            reply.settings = wb.config().cloud.clone();
            drop(wb);
            drop(admission);
            crate::publish(&app, &shared);
            Ok(CommandResult::Cloud { reply: Box::new(reply) })
        }
        Err(failure) => {
            if failure.authentication_invalidated {
                let mut wb = shared.workbench.lock().unwrap();
                if !adapter.authenticated() { wb.set_cloud_authenticated(false); }
            }
            Ok(CommandResult::CloudError { failure })
        },
    }
}

pub async fn activate(app: AppHandle, shared: State, settings: CloudSettings, confirmed: bool) -> Result<CommandResult, ApiError> {
    let generation = if settings.provider == Provider::Zhizi {
        // State never authenticates or allocates resources. It may restore a previously
        // authorized remembered token, independently from the explicit activation below.
        let result = account(app.clone(), shared.clone(), CloudCommand::State).await?;
        match result {
            CommandResult::Cloud { reply } if reply.state.authenticated => Some(reply.state.generation),
            CommandResult::CloudError { failure } => return Ok(CommandResult::CloudError { failure }),
            _ => return Ok(CommandResult::CloudError { failure: CloudFailure::new("login_required") }),
        }
    } else { None };
    let mut wb = shared.workbench.lock().unwrap();
    if generation.is_some_and(|g| !app.state::<Arc<Adapter>>().is_current(g)) {
        return Ok(CommandResult::CloudError { failure: CloudFailure::cancelled() });
    }
    let profile = wb.activate_cloud(settings, confirmed)?;
    // Read under the commit's lock: every later view shows the switch's outcome.
    let after = wb.view_seq();
    drop(wb);
    crate::publish(&app, &shared);
    Ok(CommandResult::CloudSwitch { profile, after })
}

pub async fn import_qr(app: &AppHandle) -> Result<CommandResult, ApiError> {
    let dialog = app.dialog().clone();
    tauri::async_runtime::spawn_blocking(move || {
        let Some(path) = dialog.file().add_filter("QR image", &["png", "jpg", "jpeg", "gif", "bmp"])
            .blocking_pick_file().and_then(|p| p.into_path().ok()) else {
            return Ok(CommandResult::Unchanged);
        };
        let fail = || ApiError::new(ErrorKind::InvalidArgument, "RemoteCompute.error.qrNotRecognized");
        let image = image::ImageReader::open(path).map_err(|_| fail())?
            .with_guessed_format().map_err(|_| fail())?.decode().map_err(|_| fail())?.to_luma8();
        let mut prepared = rqrr::PreparedImage::prepare(image);
        for grid in prepared.detect_grids() {
            if let Ok((_, url)) = grid.decode() {
                return Ok(CommandResult::CloudUrl { url });
            }
        }
        Err(fail())
    }).await.map_err(|_| ApiError::new(ErrorKind::Unavailable, "RemoteCompute.error.qrNotRecognized"))?
}
