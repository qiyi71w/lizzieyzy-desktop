//! 跑谱贡献 settings (`ContributeSettings` / `ContributeView`, `Config.java:2083-2119`):
//! `ui.contribute-*`, `ui.show-Contribute` and `leelaz.contribute-engine-ssh-info`.
//!
//! The training account (`contribute-user-name`, `contribute-password`, kept as typed by Java),
//! a custom command carrying `password=` and the 远程SSH password (`Utils.doEncrypt`) come over
//! only when the user confirms copying them; their values are never shown in the report.

use super::*;
use crate::contribution::command::conceal_passwords;

/// The Java training account and passwords as read; copied only when confirmed.
#[derive(Clone, Default, PartialEq)]
pub struct ContributionSecrets {
    pub user_name: String,
    pub password: String,
    /// The custom command as typed when it carries passwords.
    pub command: Option<String>,
    /// The 远程SSH connection password, when it does not use a key.
    pub ssh_password: Option<JavaSecret>,
}

impl std::fmt::Debug for ContributionSecrets {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContributionSecrets(..)")
    }
}

impl ContributionSecrets {
    fn is_empty(&self) -> bool {
        self.user_name.is_empty() && self.password.is_empty() && self.command.is_none() && self.ssh_password.is_none()
    }
}

/// What a confirmed import hands to the platform credential storage.
#[derive(Clone, PartialEq)]
pub struct ContributionCredentials {
    pub user_name: String,
    /// The training password (empty: none).
    pub password: String,
    /// The custom command as typed, whose saved form conceals its passwords.
    pub command: Option<String>,
}

impl std::fmt::Debug for ContributionCredentials {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("ContributionCredentials(..)")
    }
}

/// Report keys that hold the confirmed credentials.
const CREDENTIAL_KEYS: [&str; 4] = [
    "ui.contribute-user-name",
    "ui.contribute-password",
    "ui.contribute-command",
    "leelaz.contribute-engine-ssh-info",
];

impl Reader<'_> {
    pub(super) fn contribution(&mut self, ui: &Map<String, Value>, leelaz: &Map<String, Value>, handled: &mut Handled) {
        const TARGET: ImportTarget = ImportTarget::Contribution;
        for (key, field) in [
            ("contribute-show-estimate", "showEstimate"),
            ("contribute-disable-rating-matches", "disableRatingMatches"),
            ("contribute-use-command", "useCommand"),
            ("contribute-use-pure-command", "usePureCommand"),
            ("contribute-auto-save", "autoSave"),
            ("show-Contribute", "showMenu"),
            ("contribute-watch-always-last-move", "watch.alwaysLastMove"),
            ("contribute-watch-auto-play", "watch.autoPlay"),
            ("contribute-watch-auto-play-next-game", "watch.autoPlayNextGame"),
            ("contribute-watch-skip-none-19", "watch.skipNon19"),
            ("contribute-hide-console", "watch.hideConsole"),
            ("contribute-hide-rules", "watch.hideRules"),
            ("contribute-always-top", "watch.alwaysTop"),
        ] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            self.contribution_value(format!("ui.{key}"), field, as_bool(raw).map(Value::from), raw);
        }
        // `optString`: Java passes these texts to the contribution command as typed.
        for (key, field) in [
            ("contribute-engine-path", "enginePath"),
            ("contribute-config-path", "configPath"),
            ("contribute-batch-games", "batchGames"),
        ] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            let value = raw.as_str().map(Value::from);
            self.contribution_value(format!("ui.{key}"), field, value, raw);
        }
        // ContributeView keeps only positive seconds (DoubleDocument, `> 0`).
        if let Some(raw) = ui.get("contribute-watch-auto-play-interval") {
            handled.push(("ui", "contribute-watch-auto-play-interval"));
            let value = as_f64(raw).filter(|s| *s > 0.0).map(Value::from);
            self.contribution_value("ui.contribute-watch-auto-play-interval".into(), "watch.autoPlayInterval", value, raw);
        }

        let mut secrets = ContributionSecrets::default();
        for key in ["contribute-user-name", "contribute-password"] {
            let Some(raw) = ui.get(key) else { continue };
            handled.push(("ui", key));
            let name = format!("ui.{key}");
            let Some(text) = raw.as_str() else {
                self.invalid(name, TARGET, raw);
                continue;
            };
            if text.is_empty() {
                // Nothing to copy: an empty account is Java's default.
                if key == "contribute-user-name" {
                    self.mapped.contribution.push(DisplayValue { target: TARGET, field: "userName", value: Value::from("") });
                }
                let it = self.item(name, ImportStatus::Imported);
                it.target = Some(TARGET);
                continue;
            }
            if key == "contribute-user-name" { secrets.user_name = text.into(); } else { secrets.password = text.into(); }
            self.credential_item(name);
        }
        if let Some(raw) = ui.get("contribute-command") {
            handled.push(("ui", "contribute-command"));
            match raw.as_str() {
                Some(command) if conceal_passwords(command) != command => {
                    secrets.command = Some(command.into());
                    self.credential_item("ui.contribute-command".into());
                }
                Some(command) => self.contribution_value("ui.contribute-command".into(), "command", Some(Value::from(command)), raw),
                None => self.invalid("ui.contribute-command".into(), TARGET, raw),
            }
        }
        if let Some(raw) = leelaz.get("contribute-engine-ssh-info") {
            handled.push(("leelaz", "contribute-engine-ssh-info"));
            match raw.as_object() {
                Some(info) => {
                    let ssh = ssh_settings(info);
                    let on = info.get("useJavaSSH").and_then(as_bool).unwrap_or(false);
                    secrets.ssh_password = (!ssh.use_key).then(|| JavaSecret::read(info.get("password"))).flatten();
                    let shown = format!("{} {}:{}", if on { "on" } else { "off" }, ssh.host.trim(), ssh.port_number());
                    self.imported("leelaz.contribute-engine-ssh-info".into(), TARGET, shown);
                    self.items.last_mut().unwrap().credential = secrets.ssh_password.is_some();
                    self.mapped.contribution.push(DisplayValue { target: TARGET, field: "remote", value: Value::from(on) });
                    let ssh = serde_json::to_value(&ssh).expect("ssh settings serialize");
                    self.mapped.contribution.push(DisplayValue { target: TARGET, field: "ssh", value: ssh });
                }
                None => self.invalid("leelaz.contribute-engine-ssh-info".into(), TARGET, raw),
            }
        }
        if !secrets.is_empty() {
            self.mapped.contribution_secrets = Some(secrets);
        }
    }

    fn contribution_value(&mut self, name: String, field: &'static str, value: Option<Value>, raw: &Value) {
        match value {
            Some(value) => {
                self.imported(name, ImportTarget::Contribution, shown(&value));
                self.mapped.contribution.push(DisplayValue { target: ImportTarget::Contribution, field, value });
            }
            None => self.invalid(name, ImportTarget::Contribution, raw),
        }
    }

    /// A credential taken over once confirmed; its value is never shown.
    fn credential_item(&mut self, name: String) {
        let it = self.item(name, ImportStatus::Imported);
        it.target = Some(ImportTarget::Contribution);
        it.credential = true;
    }
}

impl Candidate {
    /// The Java import has a 跑谱贡献 account, password or SSH password to copy.
    pub fn contribution_credential(&self) -> bool {
        self.mapped.contribution_secrets.is_some()
    }

    /// The training account, password and custom command to keep once a confirmed import is
    /// applied (`None` when not confirmed or there is nothing to copy).
    pub fn contribution_credentials(&self, copy: bool) -> Option<ContributionCredentials> {
        let s = self.mapped.contribution_secrets.as_ref().filter(|_| copy)?;
        (!s.user_name.is_empty() || !s.password.is_empty() || s.command.is_some()).then(|| ContributionCredentials {
            user_name: s.user_name.clone(),
            password: s.password.clone(),
            command: s.command.clone(),
        })
    }

    /// The 远程SSH account and its password, when confirmed and Java wrote one.
    pub(super) fn contribution_ssh_password(&self, copy: bool) -> Option<(String, String)> {
        let s = self.mapped.contribution_secrets.as_ref().filter(|_| copy)?;
        let password = s.ssh_password.as_ref()?.decrypt()?;
        let ssh = self.mapped.contribution.iter().find(|v| v.field == "ssh")?;
        let ssh: SshSettings = serde_json::from_value(ssh.value.clone()).ok()?;
        Some((ssh.account(), password))
    }

    /// Report entries holding the contribution credentials once the choice is made.
    pub(super) fn contribution_after(&self, item: &mut ImportItemView, copy: bool) {
        if copy || !item.credential || !CREDENTIAL_KEYS.contains(&item.key.as_str()) {
            return;
        }
        if item.key == "leelaz.contribute-engine-ssh-info" {
            // The connection is taken over; only its password stays behind.
            item.detail = Some("password".into());
            item.reason = Some(ImportReason::CredentialNotConfirmed);
            return;
        }
        item.status = ImportStatus::NotMigrated;
        item.reason = Some(ImportReason::CredentialNotConfirmed);
    }
}

impl Mapped {
    /// [`crate::settings::ContributionSettings`] with the imported values; the account and the
    /// command carrying passwords only when `copy`. The saved command conceals its passwords.
    pub(super) fn apply_contribution(&self, settings: &mut crate::settings::ContributionSettings, copy: bool) {
        let Ok(mut json) = serde_json::to_value(&*settings) else { return };
        for value in &self.contribution {
            value.set(&mut json);
        }
        if let Some(s) = self.contribution_secrets.as_ref().filter(|_| copy) {
            if !s.user_name.is_empty() {
                json["userName"] = Value::from(s.user_name.clone());
            }
            if let Some(command) = &s.command {
                json["command"] = Value::from(conceal_passwords(command));
            }
        }
        if let Ok(next) = serde_json::from_value(json) {
            *settings = next;
        }
    }

    /// Whether importing would change the 跑谱贡献 settings chosen in this product.
    pub(super) fn replaces_contribution(&self, settings: &crate::settings::ContributionSettings) -> bool {
        let mut next = settings.clone();
        self.apply_contribution(&mut next, true);
        next != *settings
    }
}
