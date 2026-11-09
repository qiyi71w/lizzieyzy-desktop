//! 跑谱贡献's platform side (Java `ContributeSettings` + `ContributeEngine`): the training
//! password in the system credential storage, and the start hand-off that reads it for the
//! command line. Everything else is the workbench's ([`lizzie_core::api::ContributionCommand`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{
    ApiError, CommandResult, ContributionCommand, ContributionLink, ErrorKind, SshPasswordState,
};
use tauri::AppHandle;
use tauri_plugin_opener::OpenerExt;

use crate::secrets::Store;
use crate::{publish, State};

const KIND: &str = "contribution-password";
/// The custom command as typed when it carries a password (its concealed form is what is saved).
const COMMAND_KIND: &str = "contribution-command";
const COMMAND_ACCOUNT: &str = "command";

/// The training password and the custom command's passwords: system credential storage, or this
/// run only where it is unavailable (the same `Session` semantics as the SSH passwords).
pub struct Adapter {
    store: Store,
    session: Mutex<HashMap<String, String>>,
    command_session: Mutex<Option<String>>,
}

impl Adapter {
    pub fn new(application: String, config_dir: &Path) -> Arc<Adapter> {
        Arc::new(Adapter {
            store: Store::new(application, config_dir),
            session: Mutex::new(HashMap::new()),
            command_session: Mutex::new(None),
        })
    }

    fn password(&self, user: &str) -> Option<String> {
        if let Some(password) = self.session.lock().unwrap().get(user) {
            return Some(password.clone());
        }
        self.store
            .read(KIND, user)
            .ok()
            .flatten()
            .filter(|password| !password.is_empty())
    }

    fn state(&self, user: &str) -> SshPasswordState {
        if self.session.lock().unwrap().contains_key(user) {
            SshPasswordState::Session
        } else if self
            .store
            .read(KIND, user)
            .ok()
            .flatten()
            .is_some_and(|password| !password.is_empty())
        {
            SshPasswordState::Stored
        } else {
            SshPasswordState::None
        }
    }

    fn set_password(&self, user: &str, password: &str) -> SshPasswordState {
        let mut session = self.session.lock().unwrap();
        session.remove(user);
        if password.is_empty() {
            let _ = self.store.delete(KIND, user);
            return SshPasswordState::None;
        }
        match self.store.write(KIND, user, password) {
            Ok(()) => SshPasswordState::Stored,
            Err(_) => {
                session.insert(user.to_string(), password.to_string());
                SshPasswordState::Session
            }
        }
    }

    fn kept_command(&self) -> Option<String> {
        let session = self.command_session.lock().unwrap().clone();
        session
            .or_else(|| self.store.read(COMMAND_KIND, COMMAND_ACCOUNT).ok().flatten())
            .filter(|command| !command.is_empty())
    }

    /// Keeps `typed` when saving it conceals a password, and forgets the kept command when it holds
    /// none. Saving the concealed form already saved (`saved`) keeps what is kept.
    fn keep_command(&self, typed: &str, saved: &str) {
        if typed == saved {
            return;
        }
        let concealed = lizzie_core::contribution::command::conceal_passwords(typed) != typed;
        let mut session = self.command_session.lock().unwrap();
        *session = None;
        if !concealed {
            let _ = self.store.delete(COMMAND_KIND, COMMAND_ACCOUNT);
        } else if self.store.write(COMMAND_KIND, COMMAND_ACCOUNT, typed).is_err() {
            *session = Some(typed.to_string());
        }
    }
}

fn open_link(app: &AppHandle, link: ContributionLink) -> Result<CommandResult, ApiError> {
    let url = match link {
        ContributionLink::SignUp => "https://katagotraining.org/accounts/signup/",
        ContributionLink::OfficialWebsite => "https://katagotraining.org/",
    };
    app.opener()
        .open_url(url, None::<&str>)
        .map_err(|_| ApiError::new(ErrorKind::Unavailable, "AutoSetup.openBrowserUnsupported"))?;
    Ok(CommandResult::Done)
}

pub fn dispatch(
    app: &AppHandle,
    shared: &State,
    adapter: &Adapter,
    op: ContributionCommand,
) -> Result<CommandResult, ApiError> {
    match op {
        ContributionCommand::SetPassword { user, password } => {
            Ok(CommandResult::ContributionPassword {
                state: adapter.set_password(&user, &password),
            })
        }
        ContributionCommand::PasswordState { user } => Ok(CommandResult::ContributionPassword {
            state: adapter.state(&user),
        }),
        ContributionCommand::OpenLink { link } => open_link(app, link),
        ContributionCommand::SaveSettings { settings } => {
            // The credential storage can be slow; it is written without holding the workbench.
            let saved = shared.workbench.lock().unwrap().config().contribution.command.clone();
            adapter.keep_command(&settings.command, &saved);
            let result = shared
                .workbench
                .lock()
                .unwrap()
                .contribution(ContributionCommand::SaveSettings { settings })?;
            publish(app, shared);
            Ok(result)
        }
        ContributionCommand::Start => {
            // The credential storage can be slow; it is read without holding the workbench.
            let user = shared.workbench.lock().unwrap().config().contribution.user_name.clone();
            let password = adapter.password(&user);
            let kept = adapter.kept_command();
            let result = shared
                .workbench
                .lock()
                .unwrap()
                .contribution_start(password, kept.as_deref())?;
            publish(app, shared);
            Ok(result)
        }
        // 浏览 is the native chooser, owned by `lib.rs`.
        ContributionCommand::ChooseFile { .. } => Ok(CommandResult::Unchanged),
        other => {
            let result = shared.workbench.lock().unwrap().contribution(other)?;
            publish(app, shared);
            Ok(result)
        }
    }
}
