//! 跑谱贡献's platform side (Java `ContributeSettings` + `ContributeEngine`): the training
//! password in the system credential storage, and the start hand-off that reads it for the
//! command line. Everything else is the workbench's ([`lizzie_core::api::ContributionCommand`]).

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{
    ApiError, CommandResult, ContributionCommand, ContributionLink, ErrorKind, ImportItemView, ImportReason,
    ImportReportView, ImportStatus, ImportTarget, SshPasswordState,
};
use lizzie_core::java_import::ContributionCredentials;
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
    pub fn new(store: Store) -> Arc<Adapter> {
        Arc::new(Adapter {
            store,
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

    /// Keeps the custom command as typed when saving conceals a password in it, the passwords still
    /// written as saved taken from the command kept before; forgets it when it holds none.
    fn keep_command(&self, typed: &str) {
        use lizzie_core::contribution::command::{conceal_passwords, fill_passwords};
        let kept = self.kept_command();
        let command = fill_passwords(typed, kept.as_deref());
        if kept.as_deref() == Some(command.as_str()) {
            return;
        }
        let concealed = conceal_passwords(&command) != command;
        let mut session = self.command_session.lock().unwrap();
        *session = None;
        if !concealed {
            let _ = self.store.delete(COMMAND_KIND, COMMAND_ACCOUNT);
        } else if self.store.write(COMMAND_KIND, COMMAND_ACCOUNT, &command).is_err() {
            *session = Some(command);
        }
    }

    /// Keeps `command` (the custom command as typed) or forgets the kept one; `true` when it is
    /// kept for this run only.
    fn set_command(&self, command: Option<&str>) -> bool {
        let mut session = self.command_session.lock().unwrap();
        *session = None;
        match command {
            None => {
                let _ = self.store.delete(COMMAND_KIND, COMMAND_ACCOUNT);
                false
            }
            Some(command) => {
                let failed = self.store.write(COMMAND_KIND, COMMAND_ACCOUNT, command).is_err();
                if failed {
                    *session = Some(command.to_string());
                }
                failed
            }
        }
    }

    /// Keeps the confirmed Java 跑谱贡献 password and custom command before the import is
    /// applied. Returns what to report once it succeeded and what to put back if it does not.
    pub fn stage_import(&self, credentials: Option<ContributionCredentials>) -> StagedContribution {
        let mut staged = StagedContribution::default();
        let Some(c) = credentials else { return staged };
        if !c.password.is_empty() {
            let previous = self.password(&c.user_name);
            let state = self.set_password(&c.user_name, &c.password);
            staged.password = Some((c.user_name.clone(), previous, state));
        }
        if let Some(command) = &c.command {
            let previous = self.kept_command();
            let session = self.set_command(Some(command));
            staged.command = Some((previous, session));
        }
        staged
    }

    /// The import was refused or failed: the password and the command kept before come back.
    pub fn unstage_import(&self, staged: StagedContribution) {
        if let Some((previous, _)) = staged.command {
            self.set_command(previous.as_deref());
        }
        if let Some((user, previous, _)) = staged.password {
            self.set_password(&user, previous.as_deref().unwrap_or(""));
        }
    }
}

#[derive(Default)]
pub struct StagedContribution {
    /// The account, its password before, and where the imported one went.
    password: Option<(String, Option<String>, SshPasswordState)>,
    /// The command kept before, and whether the imported one is kept for this run only.
    command: Option<(Option<String>, bool)>,
}

impl StagedContribution {
    /// One report entry per credential kept: where it went, never its value.
    pub fn report(&self, report: &mut ImportReportView) {
        let mut entry = |key: &str, session: bool| {
            report.items.push(ImportItemView {
                key: key.into(),
                status: ImportStatus::Imported,
                target: Some(ImportTarget::Contribution),
                value: None,
                reason: session.then_some(ImportReason::SessionOnly),
                detail: None,
                credential: true,
            });
        };
        if let Some((_, _, state)) = &self.password {
            entry("credential:contribution-password", *state == SshPasswordState::Session);
        }
        if let Some((_, session)) = &self.command {
            entry("credential:contribution-command", *session);
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
            adapter.keep_command(&settings.command);
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
