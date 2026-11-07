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

/// The credential-storage name of an SSH account. The storage compares names without case,
/// while SSH user names keep it: `%` and every character with a lower-case form are written as
/// `%XXXX` code points, so `Alice@h:22` and `alice@h:22` stay apart. A name already in lower
/// case is unchanged.
fn store_account(account: &str) -> String {
    let mut out = String::with_capacity(account.len());
    for c in account.chars() {
        if c == '%' || c.to_lowercase().ne(std::iter::once(c)) {
            out.push_str(&format!("%{:04X}", u32::from(c)));
        } else {
            out.push(c);
        }
    }
    out
}

pub struct Adapter {
    store: Store,
    /// Passwords of this run, by account, where the system storage could not keep them.
    session: Mutex<HashMap<String, String>>,
    /// Accounts with a staged password and the password they had before.
    staged: Mutex<HashMap<String, Option<String>>>,
    known_hosts: Arc<KnownHosts>,
}

impl Adapter {
    pub fn new(application: String, config_dir: &Path) -> Arc<Adapter> {
        Arc::new(Adapter {
            store: Store::new(application, config_dir),
            session: Mutex::new(HashMap::new()),
            staged: Mutex::new(HashMap::new()),
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
        self.store
            .read(KIND, &store_account(account))
            .ok()
            .flatten()
            .filter(|p| !p.is_empty())
    }

    fn state(&self, account: &str) -> SshPasswordState {
        if self.session.lock().unwrap().contains_key(account) {
            SshPasswordState::Session
        } else if self
            .store
            .read(KIND, &store_account(account))
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
            let _ = self.store.delete(KIND, &store_account(account));
            return SshPasswordState::None;
        }
        match self.store.write(KIND, &store_account(account), password) {
            Ok(()) => SshPasswordState::Stored,
            Err(_) => {
                session.insert(account.to_string(), password.to_string());
                SshPasswordState::Session
            }
        }
    }

    /// [`SshCredentialCommand::StagePassword`]: the first stage of an account remembers what to
    /// put back.
    fn stage(&self, account: &str, password: &str) -> SshPasswordState {
        let previous = self.password(account);
        self.staged
            .lock()
            .unwrap()
            .entry(account.to_string())
            .or_insert(previous);
        self.set_password(account, password)
    }

    /// [`SshCredentialCommand::SettlePassword`].
    fn settle(&self, account: &str, keep: bool) -> SshPasswordState {
        let staged = self.staged.lock().unwrap().remove(account);
        if let (Some(previous), false) = (staged, keep) {
            self.set_password(account, previous.as_deref().unwrap_or(""));
        }
        self.state(account)
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
        SshCredentialCommand::StagePassword { ssh, password } => Ok(CommandResult::SshPassword {
            state: adapter.stage(&ssh.account(), &password),
        }),
        SshCredentialCommand::SettlePassword { ssh, keep } => Ok(CommandResult::SshPassword {
            state: adapter.settle(&ssh.account(), keep),
        }),
        SshCredentialCommand::TrustHostKey { host, port, offered } => {
            match adapter.known_hosts.trust_offered(&host, port, &offered) {
                Ok(true) => Ok(CommandResult::Done),
                Ok(false) => Ok(CommandResult::Unchanged),
                Err(e) => Err(ApiError::new(ErrorKind::Unavailable, e.to_string())),
            }
        }
        // The shell owns the native chooser (`lib.rs`).
        SshCredentialCommand::ChooseKeyFile { .. } => Ok(CommandResult::Unchanged),
    }
}

#[cfg(test)]
mod tests {
    use super::store_account;

    #[test]
    fn accounts_differing_only_in_case_keep_their_own_storage_names() {
        // The storage lower-cases names; distinct accounts must stay distinct after that.
        let stored = |account: &str| store_account(account).to_lowercase();
        assert_ne!(stored("Alice@h:22"), stored("alice@h:22"));
        assert_ne!(stored("a%0041@h:22"), stored("aA@h:22"));
        assert_eq!(
            store_account("alice@host:22"),
            "alice@host:22",
            "lower-case names are unchanged"
        );
    }
}
