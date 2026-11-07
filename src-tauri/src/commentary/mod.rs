//! Native ownership of commentary credentials, connections, request threads and sign-in.
mod http;
mod secrets;
mod chatgpt {
    pub mod identity;
    pub mod sessions;
    pub mod signin;
    pub mod transport;
}

use crate::{publish, Shared, State};
use chatgpt::{sessions, signin::SignIn, transport::Transport};
use lizzie_core::api::{
    ApiError, CommandResult, CommentaryChatGptFacts, CommentaryCommand, CommentaryConnectionFacts, CommentaryProvider,
    CommentarySecureStorage, ConnectionCommand, ErrorKind,
};
use lizzie_core::commentary::{
    api_key::ApiKeyClient, grounded, settings, CommentaryFailure, CommentaryJob, JobConnection,
};
use std::path::Path;
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc, Mutex, Weak,
};
use std::time::{Duration, Instant};
use tauri::{AppHandle, Manager};
use tauri_plugin_opener::OpenerExt;

#[derive(Default)]
struct SessionKey {
    key: String,
    restore_attempted: bool,
}
pub struct Adapter {
    key: Mutex<SessionKey>,
    store: secrets::Store,
    sessions: Mutex<sessions::Sessions>,
    sign_in: Mutex<Option<Arc<AtomicBool>>>,
    jobs: Mutex<Vec<Weak<AtomicBool>>>,
    connection_write: Mutex<()>,
}
impl Adapter {
    pub fn new(application: String, config_dir: &Path, data_dir: &Path) -> Self {
        let store = secrets::Store::new(application, config_dir);
        Self {
            key: Mutex::new(SessionKey::default()),
            sessions: Mutex::new(sessions::Sessions::new(data_dir.join("chatgpt"), store.clone())),
            store,
            sign_in: Mutex::new(None),
            jobs: Mutex::new(Vec::new()),
            connection_write: Mutex::new(()),
        }
    }
    fn cancel_sign_in(&self) {
        if let Some(cancel) = self.sign_in.lock().unwrap().take() {
            cancel.store(true, Ordering::Release);
        }
    }
    fn cancel_jobs(&self) {
        let mut jobs = self.jobs.lock().unwrap();
        for job in jobs.iter().filter_map(Weak::upgrade) {
            job.store(true, Ordering::Release);
        }
        jobs.clear();
    }
    fn restore_key(&self, settings: &settings::CommentarySettings) -> Result<(), CommentaryFailure> {
        let mut session = self.key.lock().unwrap();
        if session.restore_attempted
            || !session.key.is_empty()
            || !settings.remember_api_key
            || settings.base_url.is_empty()
        {
            return Ok(());
        }
        session.restore_attempted = true;
        let account = settings::credential_account(&settings.base_url)?;
        session.key = self
            .store
            .read("api-key", &account)
            .map_err(|_| storage_failure())?
            .unwrap_or_default();
        Ok(())
    }
}
fn storage_failure() -> CommentaryFailure {
    CommentaryFailure::keyed(
        "Teacher.settings.storageUnavailable",
        "System credential storage is unavailable; the key will be session-only.",
    )
}
fn api_error(failure: CommentaryFailure) -> ApiError {
    ApiError::new(
        ErrorKind::InvalidArgument,
        if failure.message_key.is_empty() {
            failure.message
        } else {
            failure.message_key
        },
    )
}
fn network(shared: &Shared) -> lizzie_core::settings::NetworkSettings {
    shared.workbench.lock().unwrap().config().network.clone()
}
fn invalidate(shared: &Shared, adapter: &Adapter) {
    adapter.cancel_jobs();
    let _ = shared.workbench.lock().unwrap().commentary(CommentaryCommand::Stop);
}

pub fn push_facts(app: &AppHandle, shared: &Shared) {
    let adapter = app.state::<Adapter>();
    let signing_in = adapter
        .sign_in
        .lock()
        .unwrap()
        .as_ref()
        .is_some_and(|flag| !flag.load(Ordering::Acquire));
    let chatgpt = adapter
        .sessions
        .lock()
        .unwrap()
        .facts(signing_in)
        .unwrap_or_else(|error| CommentaryChatGptFacts {
            signing_in,
            status_key: error.message_key,
            ..CommentaryChatGptFacts::default()
        });
    let facts = CommentaryConnectionFacts {
        loaded: true,
        settings_usable: true,
        secure_storage: CommentarySecureStorage {
            available: adapter.store.available(),
            backend: adapter.store.backend().into(),
        },
        api_key_present: !adapter.key.lock().unwrap().key.is_empty(),
        chatgpt,
    };
    shared.workbench.lock().unwrap().set_commentary_facts(facts);
}

/// Called off the workbench lock and the UI thread after the page sends Open.
pub fn opened(app: &AppHandle, shared: &Shared) {
    let settings = shared.workbench.lock().unwrap().config().commentary.clone();
    let adapter = app.state::<Adapter>();
    if settings.provider != CommentaryProvider::ChatGpt {
        let _ = adapter.restore_key(&settings);
    }
    push_facts(app, shared);
    if settings.provider == CommentaryProvider::ChatGpt {
        let _ = refresh_models(app, shared);
        push_facts(app, shared);
    }
    publish(app, shared);
}

pub fn closed(app: &AppHandle, shared: &Shared) {
    let adapter = app.state::<Adapter>();
    adapter.cancel_sign_in();
    adapter.cancel_jobs();
    let _ = shared.workbench.lock().unwrap().commentary(CommentaryCommand::Close);
    // Never perform secure-store IO from a native window callback.
    let app = app.clone();
    let shared = app.state::<State>().inner().clone();
    std::thread::spawn(move || {
        push_facts(&app, &shared);
        publish(&app, &shared);
    });
}
pub fn shutdown(app: &AppHandle) {
    let adapter = app.state::<Adapter>();
    adapter.cancel_sign_in();
    adapter.cancel_jobs();
    adapter.key.lock().unwrap().key.clear();
    // A cancelled HTTP worker must not delay application shutdown behind a store/network lock.
    if let Ok(mut sessions) = adapter.sessions.try_lock() {
        sessions.clear_memory();
    };
}

pub fn dispatch(app: &AppHandle, shared: &Shared, operation: ConnectionCommand) -> Result<CommandResult, ApiError> {
    let result = dispatch_inner(app, shared, operation);
    push_facts(app, shared);
    publish(app, shared);
    result.map_err(api_error)
}
fn dispatch_inner(
    app: &AppHandle,
    shared: &Shared,
    operation: ConnectionCommand,
) -> Result<CommandResult, CommentaryFailure> {
    let adapter = app.state::<Adapter>();
    match operation {
        ConnectionCommand::ListApiModels { base_url, api_key } => {
            let mut client = ApiKeyClient::for_models(&base_url, &api_key)?.with_exchange(
                http::Http::new(&network(shared), sessions::fresh_cancel(), false)
                    .map_err(|e| CommentaryFailure::new(e.message))?,
            );
            return Ok(CommandResult::CommentaryModels {
                models: client.list_models()?,
            });
        }
        ConnectionCommand::SaveApiKeyConnection {
            base_url,
            model,
            api_key,
            remember,
        } => {
            let _write = adapter.connection_write.lock().unwrap();
            let base_url = settings::validate_base_url(&base_url)?;
            let model = settings::validate_model(&model)?;
            let previous = shared.workbench.lock().unwrap().config().commentary.clone();
            let account = settings::credential_account(&base_url)?;
            if remember {
                if api_key.is_empty() {
                    return Err(CommentaryFailure::keyed(
                        "Teacher.status.needsKey",
                        "An API key is required before it can be remembered.",
                    ));
                }
                adapter
                    .store
                    .write("api-key", &account, &api_key)
                    .map_err(|_| storage_failure())?;
            } else {
                adapter
                    .store
                    .delete("api-key", &account)
                    .map_err(|_| storage_failure())?;
            }
            if let Ok(old) = settings::credential_account(&previous.base_url) {
                if old != account {
                    adapter.store.delete("api-key", &old).map_err(|_| storage_failure())?;
                }
            }
            // Password input is authoritative, including an intentionally empty field.
            adapter.key.lock().unwrap().key = api_key;
            shared
                .workbench
                .lock()
                .unwrap()
                .set_commentary_connection(CommentaryProvider::ApiKey, base_url, model, remember)
                .map_err(|_| {
                    CommentaryFailure::keyed("Teacher.error.generic", "Could not save commentary settings.")
                })?;
        }
        ConnectionCommand::ForgetApiKey => {
            let _write = adapter.connection_write.lock().unwrap();
            let settings = shared.workbench.lock().unwrap().config().commentary.clone();
            if let Ok(account) = settings::credential_account(&settings.base_url) {
                adapter
                    .store
                    .delete("api-key", &account)
                    .map_err(|_| storage_failure())?;
            }
            adapter.key.lock().unwrap().key.clear();
            shared
                .workbench
                .lock()
                .unwrap()
                .set_commentary_connection(settings.provider, settings.base_url, settings.model, false)
                .map_err(|_| {
                    CommentaryFailure::keyed("Teacher.error.generic", "Could not save commentary settings.")
                })?;
        }
        ConnectionCommand::OpenApiKeyTab => {
            adapter.cancel_sign_in();
            let settings = shared.workbench.lock().unwrap().config().commentary.clone();
            adapter.restore_key(&settings)?;
        }
        ConnectionCommand::SelectProvider { provider } => {
            let _write = adapter.connection_write.lock().unwrap();
            adapter.cancel_sign_in();
            invalidate(shared, &adapter);
            let settings = shared.workbench.lock().unwrap().config().commentary.clone();
            shared
                .workbench
                .lock()
                .unwrap()
                .set_commentary_connection(provider, settings.base_url, settings.model, settings.remember_api_key)
                .map_err(|_| {
                    CommentaryFailure::keyed("Teacher.error.generic", "Could not save commentary settings.")
                })?;
        }
        ConnectionCommand::ChatGptSignIn { account } => {
            let mut pending = adapter.sign_in.lock().unwrap();
            if pending.as_ref().is_some_and(|flag| !flag.load(Ordering::Acquire)) {
                return Ok(CommandResult::Unchanged);
            }
            let cancel = sessions::fresh_cancel();
            let context = adapter.sessions.lock().unwrap().sign_in_context(account)?;
            let signin = SignIn::new(context, cancel.clone())?;
            signin.open_browser(app)?;
            *pending = Some(cancel.clone());
            drop(pending);
            let app = app.clone();
            let shared = app.state::<State>().inner().clone();
            let settings = network(&shared);
            let watchdog_app = app.clone();
            let watchdog_cancel = cancel.clone();
            std::thread::spawn(move || {
                let deadline = Instant::now() + Duration::from_secs(300);
                while !watchdog_cancel.load(Ordering::Acquire) && Instant::now() < deadline {
                    std::thread::sleep(Duration::from_millis(50));
                }
                if Instant::now() >= deadline {
                    let adapter = watchdog_app.state::<Adapter>();
                    let pending = adapter.sign_in.lock().unwrap();
                    if pending.as_ref().is_some_and(|flag| Arc::ptr_eq(flag, &watchdog_cancel)) {
                        watchdog_cancel.store(true, Ordering::Release);
                    }
                }
            });
            std::thread::spawn(move || {
                let outcome = http::Http::new(&settings, cancel.clone(), true)
                    .map_err(|_| lizzie_core::commentary::chatgpt::failure("network"))
                    .and_then(|mut http| signin.run(&app, &mut http));
                let adapter = app.state::<Adapter>();
                let mut pending = adapter.sign_in.lock().unwrap();
                if !pending.as_ref().is_some_and(|flag| Arc::ptr_eq(flag, &cancel)) {
                    return;
                }
                *pending = None;
                drop(pending);
                let timed_out = cancel.load(Ordering::Acquire);
                cancel.store(true, Ordering::Release);
                match outcome {
                    Ok(()) => {
                        invalidate(&shared, &adapter);
                        let _ = refresh_models(&app, &shared);
                    }
                    Err(error) => {
                        adapter.sessions.lock().unwrap().status_key = if timed_out && error.cancelled {
                            "Teacher.chatgpt.error.timeout".into()
                        } else {
                            error.message_key
                        };
                    }
                }
                push_facts(&app, &shared);
                publish(&app, &shared);
            });
        }
        ConnectionCommand::ChatGptCancelSignIn => adapter.cancel_sign_in(),
        ConnectionCommand::ChatGptSelectAccount { account } => {
            adapter.cancel_sign_in();
            invalidate(shared, &adapter);
            adapter.sessions.lock().unwrap().select(&account)?;
            refresh_models(app, shared)?;
        }
        ConnectionCommand::ChatGptSignOut { account } => {
            adapter.cancel_sign_in();
            invalidate(shared, &adapter);
            let mut http = http::Http::new(&network(shared), sessions::fresh_cancel(), true)
                .map_err(|_| lizzie_core::commentary::chatgpt::failure("network"))?;
            let mut sessions = adapter.sessions.lock().unwrap();
            let revoked = sessions.sign_out(&account, &mut http)?;
            sessions.status_key = if revoked { "" } else { "Teacher.chatgpt.revocation" }.into();
        }
        ConnectionCommand::ChatGptRefreshModels => refresh_models(app, shared)?,
        ConnectionCommand::SaveChatGptConnection { account, model, effort } => {
            let _write = adapter.connection_write.lock().unwrap();
            invalidate(shared, &adapter);
            adapter
                .sessions
                .lock()
                .unwrap()
                .save_connection(&account, &model, effort.as_deref())?;
            let settings = shared.workbench.lock().unwrap().config().commentary.clone();
            shared
                .workbench
                .lock()
                .unwrap()
                .set_commentary_connection(
                    CommentaryProvider::ChatGpt,
                    settings.base_url,
                    settings.model,
                    settings.remember_api_key,
                )
                .map_err(|_| {
                    CommentaryFailure::keyed("Teacher.error.generic", "Could not save commentary settings.")
                })?;
        }
        ConnectionCommand::OpenUsage => {
            app.opener()
                .open_url("https://chatgpt.com/settings/usage", None::<&str>)
                .map_err(|_| lizzie_core::commentary::chatgpt::failure("browser"))?;
        }
    }
    Ok(CommandResult::Done)
}

fn refresh_models(app: &AppHandle, shared: &Shared) -> Result<(), CommentaryFailure> {
    let adapter = app.state::<Adapter>();
    let mut http = http::Http::new(&network(shared), sessions::fresh_cancel(), true)
        .map_err(|_| lizzie_core::commentary::chatgpt::failure("network"))?;
    let mut sessions = adapter.sessions.lock().unwrap();
    let result: Result<(), CommentaryFailure> = (|| {
        let account = sessions
            .active()?
            .ok_or_else(|| lizzie_core::commentary::chatgpt::failure("loginRequired"))?;
        let token = sessions.access_token(&account, &mut http)?;
        sessions.models = chatgpt::transport::models(&mut http, &token)?;
        Ok(())
    })();
    sessions.status_key = match &result {
        Ok(()) => sessions.refreshed_status()?,
        Err(error) => error.message_key.clone(),
    };
    result
}

pub fn spawn_jobs(app: &AppHandle, shared: &Shared, jobs: Vec<CommentaryJob>) {
    if jobs.is_empty() {
        return;
    }
    let settings = network(shared);
    for job in jobs {
        let app = app.clone();
        let shared = app.state::<State>().inner().clone();
        let settings = settings.clone();
        let adapter = app.state::<Adapter>();
        {
            let mut jobs = adapter.jobs.lock().unwrap();
            jobs.retain(|flag| flag.strong_count() > 0);
            jobs.push(Arc::downgrade(&job.cancel));
        }
        std::thread::spawn(move || {
            let outcome = (|| {
                sessions::cancelled(&job.cancel)?;
                match &job.connection {
                    JobConnection::ApiKey { base_url, model } => {
                        let key = app.state::<Adapter>().key.lock().unwrap().key.clone();
                        if key.is_empty() {
                            return Err(CommentaryFailure::keyed(
                                "Teacher.status.needsKey",
                                "Configure an API key before use",
                            ));
                        }
                        let http = http::Http::new(&settings, job.cancel.clone(), false)
                            .map_err(|e| CommentaryFailure::new(e.message))?;
                        let mut transport = ApiKeyClient::new(base_url, &key, model)?.with_exchange(http);
                        grounded::run(&mut transport, &job)
                    }
                    JobConnection::ChatGpt { account, model, effort } => {
                        let mut transport = Transport::new(
                            app.clone(),
                            &settings,
                            job.cancel.clone(),
                            account.clone(),
                            model.clone(),
                            effort.clone(),
                        )?;
                        grounded::run(&mut transport, &job)
                    }
                }
            })();
            if job.cancel.load(Ordering::Acquire) {
                return;
            }
            let admitted = shared.workbench.lock().unwrap().commentary_finished(job.id, outcome);
            if admitted {
                push_facts(&app, &shared);
                publish(&app, &shared);
            }
        });
    }
}

/// Explicit transient password draft IPC. It never enters a view, config, log or core state.
#[tauri::command]
pub fn commentary_api_key_draft(app: AppHandle, window: tauri::WebviewWindow) -> Result<String, ApiError> {
    if window.label() != "commentary" {
        return Err(ApiError::new(
            ErrorKind::Unavailable,
            "This credential draft is available only in AI commentary settings.",
        ));
    }
    Ok(app.state::<Adapter>().key.lock().unwrap().key.clone())
}

/// The preview advertises only a source and address, never a credential value.
pub fn discover_java_credential(app: &AppHandle, candidate: &mut lizzie_core::java_import::Candidate) {
    if candidate.commentary_credential.is_some() {
        return;
    }
    let Some(settings) = candidate.mapped.commentary.as_ref() else {
        return;
    };
    if !settings.remember_api_key {
        return;
    }
    let Ok(account) = settings::credential_account(&settings.base_url) else {
        return;
    };
    if app
        .state::<Adapter>()
        .store
        .read_java_api_key(&candidate.source.config_dir, &account)
        .ok()
        .flatten()
        .is_some_and(|key| !key.is_empty())
    {
        candidate.set_commentary_credential("secure-store");
    }
}

pub fn imported_settings(app: &AppHandle) {
    let adapter = app.state::<Adapter>();
    let mut key = adapter.key.lock().unwrap();
    key.key.clear();
    key.restore_attempted = false;
}

pub fn copy_java_credential(
    app: &AppHandle,
    shared: &Shared,
    plan: &(
        lizzie_core::java_import::JavaSource,
        lizzie_core::api::ImportCommentaryCredential,
    ),
    report: &mut lizzie_core::api::ImportReportView,
) {
    use lizzie_core::api::{ImportReason, ImportStatus};
    let adapter = app.state::<Adapter>();
    let _write = adapter.connection_write.lock().unwrap();
    let (source, choice) = plan;
    let outcome = (|| {
        let account = settings::credential_account(&choice.base_url)?;
        let key = if choice.source == "teacher.properties" {
            lizzie_core::java_import::read_java_commentary_api_key(&source.config_dir)
        } else {
            adapter
                .store
                .read_java_api_key(&source.config_dir, &account)
                .map_err(|_| storage_failure())?
        }
        .filter(|key| !key.is_empty())
        .ok_or_else(storage_failure)?;
        adapter
            .store
            .write("api-key", &account, &key)
            .map_err(|_| storage_failure())?;
        shared
            .workbench
            .lock()
            .unwrap()
            .remember_imported_commentary_key()
            .map_err(|_| {
                CommentaryFailure::keyed("Teacher.error.generic", "Could not save imported commentary settings.")
            })?;
        Ok::<(), CommentaryFailure>(())
    })();
    if let Some(item) = report
        .items
        .iter_mut()
        .find(|item| item.key == "credential:commentary-api-key")
    {
        match outcome {
            Ok(()) => {
                item.status = ImportStatus::Imported;
                item.reason = None;
                item.detail = None;
            }
            Err(error) => {
                item.status = ImportStatus::Incompatible;
                item.reason = Some(ImportReason::Unreadable);
                item.detail = Some(error.message);
            }
        }
    }
}
