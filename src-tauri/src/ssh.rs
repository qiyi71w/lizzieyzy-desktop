//! SSH credentials of remote engines. Passwords go to the system credential storage (Windows
//! DPAPI, macOS Keychain, Linux Secret Service), or are kept for this run only where it is
//! unavailable; they never reach the settings, the workbench or the UI. Host keys are trusted on
//! first use in this application's settings folder ([`KnownHosts`]).

use std::collections::HashMap;
use std::path::Path;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{
    ApiError, CommandResult, ErrorKind, ImportReason, ImportReportView, ImportStatus, SshCredentialCommand,
    SshPasswordState,
};
use lizzie_core::engine::ssh::{KnownHosts, SshConnector};

use crate::secrets::Store;

const KIND: &str = "ssh-password";

pub struct Adapter {
    store: Store,
    /// Passwords of this run, by account, where the system storage could not keep them.
    session: Mutex<HashMap<String, String>>,
    known_hosts: Arc<KnownHosts>,
}

impl Adapter {
    pub fn new(application: String, config_dir: &Path) -> Arc<Adapter> {
        Arc::new(Adapter {
            store: Store::new(application, config_dir),
            session: Mutex::new(HashMap::new()),
            known_hosts: Arc::new(KnownHosts::new(config_dir.join("ssh-known-hosts.json"))),
        })
    }

    /// The connector the engine launcher uses: it asks this adapter for passwords.
    pub fn connector(self: &Arc<Self>) -> SshConnector {
        let adapter = self.clone();
        SshConnector::new(
            Arc::new(move |account| adapter.password(account)),
            self.known_hosts.clone(),
        )
    }

    fn password(&self, account: &str) -> Option<String> {
        if let Some(password) = self.session.lock().unwrap().get(account) {
            return Some(password.clone());
        }
        self.store.read(KIND, account).ok().flatten().filter(|p| !p.is_empty())
    }

    fn state(&self, account: &str) -> SshPasswordState {
        if self.session.lock().unwrap().contains_key(account) {
            SshPasswordState::Session
        } else if self
            .store
            .read(KIND, account)
            .ok()
            .flatten()
            .is_some_and(|p| !p.is_empty())
        {
            SshPasswordState::Stored
        } else {
            SshPasswordState::None
        }
    }

    /// Remembers `password` for `account` (an empty one forgets it): in the system storage,
    /// else for this run.
    fn set_password(&self, account: &str, password: &str) -> SshPasswordState {
        let mut session = self.session.lock().unwrap();
        session.remove(account);
        if password.is_empty() {
            let _ = self.store.delete(KIND, account);
            return SshPasswordState::None;
        }
        match self.store.write(KIND, account, password) {
            Ok(()) => SshPasswordState::Stored,
            Err(_) => {
                session.insert(account.to_string(), password.to_string());
                SshPasswordState::Session
            }
        }
    }

    /// Remembers the SSH passwords of a Java import before it is applied, so the imported engines
    /// find them when they start. Returns what to report once the import succeeded and what to
    /// put back if it does not.
    pub fn stage_import(&self, passwords: Vec<(String, String)>) -> StagedPasswords {
        let mut staged = StagedPasswords::default();
        for (account, password) in passwords {
            staged.previous.push((account.clone(), self.password(&account)));
            let state = self.set_password(&account, &password);
            staged.states.push((account, state));
        }
        staged
    }

    /// The import was refused or failed: the accounts get their previous passwords back.
    pub fn unstage_import(&self, staged: StagedPasswords) {
        for (account, previous) in staged.previous.into_iter().rev() {
            self.set_password(&account, previous.as_deref().unwrap_or(""));
        }
    }
}

#[derive(Default)]
pub struct StagedPasswords {
    previous: Vec<(String, Option<String>)>,
    states: Vec<(String, SshPasswordState)>,
}

impl StagedPasswords {
    /// One report entry per account: where its password went.
    pub fn report(&self, report: &mut ImportReportView) {
        for (account, state) in &self.states {
            // `user@host:port`: the report names the host only, like other credentials.
            let host = account.rsplit_once('@').map_or(account.as_str(), |(_, host)| host);
            report.items.push(lizzie_core::api::ImportItemView {
                key: format!("credential:ssh-password:{host}"),
                status: ImportStatus::Imported,
                target: Some(lizzie_core::api::ImportTarget::Engines),
                value: None,
                reason: (*state == SshPasswordState::Session).then_some(ImportReason::SessionOnly),
                detail: None,
                credential: true,
            });
        }
    }
}

pub fn dispatch(adapter: &Adapter, op: SshCredentialCommand) -> Result<CommandResult, ApiError> {
    match op {
        SshCredentialCommand::PasswordState { ssh } => Ok(CommandResult::SshPassword {
            state: adapter.state(&ssh.account()),
        }),
        SshCredentialCommand::SetPassword { ssh, password } => Ok(CommandResult::SshPassword {
            state: adapter.set_password(&ssh.account(), &password),
        }),
        SshCredentialCommand::TrustHostKey { host, port } => match adapter.known_hosts.trust_offered(&host, port) {
            Ok(true) => Ok(CommandResult::Done),
            Ok(false) => Ok(CommandResult::Unchanged),
            Err(e) => Err(ApiError::new(ErrorKind::Unavailable, e.to_string())),
        },
        // The shell owns the native chooser (`lib.rs`).
        SshCredentialCommand::ChooseKeyFile { .. } => Ok(CommandResult::Unchanged),
    }
}
