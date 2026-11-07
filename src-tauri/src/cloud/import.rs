use super::*;

struct NativeSecrets {
    account: String,
    token: Option<String>,
    password: Option<String>,
    readable: bool,
}
/// Linear transaction handle: native snapshots and session credentials are never serialized or
/// displayed. The caller must settle this handle on every apply outcome.
#[must_use = "settle the cloud import after settings apply succeeds or fails"]
pub struct CloudImportStage {
    previous: Session,
    native: Vec<NativeSecrets>,
    settings: CloudSettings,
}
impl CloudImportStage {
    pub fn settings(&self) -> &CloudSettings { &self.settings }
}
impl std::fmt::Debug for CloudImportStage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result { f.write_str("CloudImportStage([redacted])") }
}
/// Only restoration failures belong to the import transaction's failure result. An account
/// warning that already existed remains visible in the reply without failing this rollback.
pub struct CloudImportSettlement {
    pub reply: CloudReply,
    pub rollback_failed: bool,
}
impl Adapter {
    /// Stages consented secrets before the imported engine can start. Unreadable native entries
    /// are not overwritten: without a snapshot an honest rollback cannot restore them.
    pub fn stage_import(&self, settings: &CloudSettings, token: Option<String>, password: Option<String>) -> Result<CloudImportStage,CloudFailure> {
        let mut session = self.session.lock().unwrap();
        if session.import_pending { return Err(CloudFailure::new("import_in_progress")); }
        let mut native = Vec::new();
        for account in [&session.identifier, &settings.identifier] {
            if native.iter().any(|old: &NativeSecrets| same_account(&old.account,account)) { continue; }
            let token = self.store.read("account-token",account);
            let password = self.store.read("password",account);
            let readable = token.is_ok() && password.is_ok();
            native.push(NativeSecrets { account: account.trim().into(), token: token.ok().flatten(), password: password.ok().flatten(), readable });
        }
        self.generation.fetch_add(1,Ordering::AcqRel);
        let previous = std::mem::take(&mut *session);
        *session = Session { identifier: settings.identifier.trim().into(), restored: true, import_pending: true,
            token: token.unwrap_or_default(), password: password.unwrap_or_default(), ..Session::default() };
        let mut settings = settings.clone();
        let mut warning = false;
        for old in &native {
            if !old.readable { warning = true; continue; }
            if !same_account(&old.account,&session.identifier) {
                warning |= self.store.delete("account-token",&old.account).is_err();
                warning |= self.store.delete("password",&old.account).is_err();
            }
        }
        if native.iter().any(|old| same_account(&old.account,&session.identifier) && old.readable) {
            self.persist(&mut settings,&mut session);
        } else {
            settings.remember_token = false;
            settings.remember_password = false;
            warning = true;
        }
        session.storage_warning |= warning;
        session.settings_snapshot = settings.clone();
        Ok(CloudImportStage { previous,native,settings })
    }
    /// `keep=false` restores the native identity and every readable native credential snapshot.
    /// Failed restoration is distinct from a pre-existing account storage warning.
    pub fn settle_import(&self, stage: CloudImportStage, keep: bool) -> CloudImportSettlement {
        let CloudImportStage { mut previous,native,mut settings } = stage;
        let mut session = self.session.lock().unwrap();
        let mut rollback_failed = false;
        if keep {
            session.import_pending = false;
            settings.remember_token &= session.token_stored;
            settings.remember_password &= session.password_stored;
            session.settings_snapshot = settings.clone();
        } else {
            self.generation.fetch_add(1,Ordering::AcqRel);
            for old in native {
                if !old.readable { continue; }
                for (kind,value) in [("account-token",old.token),("password",old.password)] {
                    let restored = if let Some(value) = value {self.store.write(kind,&old.account,&value)} else {self.store.delete(kind,&old.account)};
                    if restored.is_err() {
                        rollback_failed = true;
                        if same_account(&old.account,&previous.identifier) {
                            if kind == "account-token" {previous.token_stored = false;} else {previous.password_stored = false;}
                        }
                    }
                }
            }
            previous.storage_warning |= rollback_failed;
            previous.import_pending = false;
            settings = previous.settings_snapshot.clone();
            settings.remember_token &= previous.token_stored;
            settings.remember_password &= previous.password_stored;
            *session = previous;
        }
        CloudImportSettlement { reply: CloudReply {settings,state:self.state(&session),result:CloudResult::State}, rollback_failed }
    }
}
