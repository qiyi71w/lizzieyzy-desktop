//! Cross-process account metadata; all bearer credentials live in the OS store or memory.
use std::collections::BTreeMap;
use std::fs::{File, OpenOptions};
use std::io::{Read, Write};
use std::path::{Path, PathBuf};
use std::sync::{
    atomic::{AtomicBool, Ordering},
    Arc,
};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine};
use fs2::FileExt;
use lizzie_core::api::{CommentaryAccount, CommentaryChatGptFacts, CommentaryModel};
use lizzie_core::commentary::{
    api_key::{HttpExchange, HttpReply},
    chatgpt::{self, failure},
    CommentaryFailure,
};
use ring::rand::{SecureRandom, SystemRandom};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::identity;
use crate::commentary::{http::Http, secrets::Store};

pub const AUTH: &str = "https://auth.openai.com";
pub const RESOURCE: &str = "https://api.openai.com/v1";
const SCOPE: &str = "chatgpt.tokens.use.direct";

pub fn now() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
}
pub fn random() -> Result<String, CommentaryFailure> {
    let mut bytes = [0u8; 32];
    SystemRandom::new().fill(&mut bytes).map_err(|_| failure("protocol"))?;
    Ok(URL_SAFE_NO_PAD.encode(bytes))
}
fn uuid() -> Result<String, CommentaryFailure> {
    let mut bytes = [0u8; 16];
    SystemRandom::new().fill(&mut bytes).map_err(|_| failure("storage"))?;
    bytes[6] = (bytes[6] & 15) | 64;
    bytes[8] = (bytes[8] & 63) | 128;
    let hex: String = bytes.iter().map(|b| format!("{b:02x}")).collect();
    Ok(format!(
        "{}-{}-{}-{}-{}",
        &hex[..8],
        &hex[8..12],
        &hex[12..16],
        &hex[16..20],
        &hex[20..]
    ))
}

#[derive(Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct Account {
    client: String,
    subject: String,
    email: String,
    #[serde(default = "signed_out_default")]
    signed_out: bool,
    #[serde(default)]
    model: String,
    #[serde(default)]
    welcomed: bool,
    #[serde(default)]
    session_only: bool,
    #[serde(default)]
    authorized: bool,
    #[serde(default)]
    reasoning_by_model: BTreeMap<String, String>,
}
fn signed_out_default() -> bool {
    true
}
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct State {
    host: String,
    accounts: BTreeMap<String, Account>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pending_registration: Option<String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    active: Option<String>,
    #[serde(default)]
    authorization_revision: u64,
}
#[derive(Clone, Default, Serialize, Deserialize)]
#[serde(default)]
struct Tokens {
    access_token: String,
    expires_at: u64,
    scope: String,
    refresh_token: String,
    id_token: String,
}
impl Tokens {
    fn authorized(&self) -> bool {
        self.scope.split_whitespace().any(|s| s == SCOPE)
    }
    fn normalize(reply: &Value, previous: Option<&Self>) -> Result<Self, CommentaryFailure> {
        let text = |name: &str| reply[name].as_str().unwrap_or("").to_owned();
        let access = text("access_token");
        let id = text("id_token");
        let scope = reply["scope"]
            .as_str()
            .map(str::to_owned)
            .unwrap_or_else(|| previous.map(|t| t.scope.clone()).unwrap_or_default());
        if previous.is_none()
            && access.trim().is_empty()
            && !scope.split_whitespace().any(|s| s == SCOPE)
            && !id.trim().is_empty()
        {
            return Ok(Self {
                id_token: id,
                scope,
                ..Self::default()
            });
        }
        let seconds = reply["expires_in"].as_u64().unwrap_or(0);
        if access.trim().is_empty()
            || access.chars().any(char::is_control)
            || !text("token_type").eq_ignore_ascii_case("Bearer")
            || !(1..=2_592_000).contains(&seconds)
        {
            return Err(failure("protocol"));
        }
        Ok(Self {
            access_token: access,
            expires_at: now() + seconds,
            scope,
            refresh_token: reply["refresh_token"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| previous.map(|p| p.refresh_token.clone()).unwrap_or_default()),
            id_token: reply["id_token"]
                .as_str()
                .map(str::to_owned)
                .unwrap_or_else(|| previous.map(|p| p.id_token.clone()).unwrap_or_default()),
        })
    }
}

pub struct SignInContext {
    pub profile: Option<String>,
    pub client: String,
    pub host: String,
    pub subject: Option<String>,
    pub revision: u64,
    pub reconsent: bool,
}

pub struct Sessions {
    directory: PathBuf,
    store: Store,
    memory: BTreeMap<String, Tokens>,
    pub generation: u64,
    pub models: Vec<chatgpt::Model>,
    pub status_key: String,
}
impl Sessions {
    pub fn new(directory: PathBuf, store: Store) -> Self {
        Self {
            directory,
            store,
            memory: BTreeMap::new(),
            generation: 0,
            models: Vec::new(),
            status_key: String::new(),
        }
    }
    fn locked<T>(
        &mut self,
        operation: impl FnOnce(&mut Self, &mut State) -> Result<T, CommentaryFailure>,
    ) -> Result<T, CommentaryFailure> {
        std::fs::create_dir_all(&self.directory).map_err(|_| failure("storage"))?;
        restrict(&self.directory, true)?;
        let lock_path = self.directory.join("session.lock");
        let lock = OpenOptions::new()
            .create(true)
            .truncate(false)
            .write(true)
            .open(&lock_path)
            .map_err(|_| failure("storage"))?;
        restrict(&lock_path, false)?;
        let deadline = Instant::now() + Duration::from_secs(90);
        loop {
            match lock.try_lock_exclusive() {
                Ok(()) => break,
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock && Instant::now() < deadline => {
                    std::thread::sleep(Duration::from_millis(50))
                }
                Err(e) if e.kind() == std::io::ErrorKind::WouldBlock => return Err(failure("busy")),
                Err(_) => return Err(failure("storage")),
            }
        }
        let path = self.directory.join("accounts.json");
        let mut state = if path.exists() {
            let file = File::open(path).map_err(|_| failure("storage"))?;
            let mut bytes = Vec::new();
            file.take(2_097_153)
                .read_to_end(&mut bytes)
                .map_err(|_| failure("storage"))?;
            if bytes.len() > 2_097_152 {
                return Err(failure("storage"));
            }
            let state: State = serde_json::from_slice(&bytes).map_err(|_| failure("storage"))?;
            if state.host.trim().is_empty() {
                return Err(failure("storage"));
            }
            state
        } else {
            let state = State {
                host: format!("urn:uuid:{}", uuid()?),
                accounts: BTreeMap::new(),
                pending_registration: None,
                active: None,
                authorization_revision: 0,
            };
            self.persist(&state)?;
            state
        };
        operation(self, &mut state)
        // Dropping the file releases the process lock even on failure.
    }
    fn persist(&self, state: &State) -> Result<(), CommentaryFailure> {
        let temp = self.directory.join(format!("accounts-{}.tmp", random()?));
        let result = (|| {
            let mut file = OpenOptions::new()
                .create_new(true)
                .write(true)
                .open(&temp)
                .map_err(|_| failure("storage"))?;
            restrict(&temp, false)?;
            serde_json::to_writer_pretty(&mut file, state).map_err(|_| failure("storage"))?;
            file.flush()
                .and_then(|_| file.sync_all())
                .map_err(|_| failure("storage"))?;
            drop(file);
            let target = self.directory.join("accounts.json");
            if std::fs::rename(&temp, &target).is_err() {
                std::fs::copy(&temp, &target).map_err(|_| failure("storage"))?;
                restrict(&target, false)?;
            }
            Ok(())
        })();
        let _ = std::fs::remove_file(&temp);
        result
    }
    fn key(state: &State, id: &str) -> String {
        format!("chatgpt:{}:{id}", state.host)
    }
    fn tokens(&self, state: &State, id: &str) -> Result<Tokens, CommentaryFailure> {
        let account = state.accounts.get(id).ok_or_else(|| failure("loginRequired"))?;
        if account.signed_out {
            return Err(failure("loginRequired"));
        }
        if account.session_only {
            return self.memory.get(id).cloned().ok_or_else(|| failure("loginRequired"));
        }
        if !self.store.available() {
            return Err(failure("credentialsUnavailable"));
        }
        let value = self
            .store
            .read("chatgpt-session", &Self::key(state, id))
            .map_err(|_| failure("credentialsUnavailable"))?
            .ok_or_else(|| failure("loginRequired"))?;
        serde_json::from_str(&value).map_err(|_| failure("loginRequired"))
    }
    fn save_tokens(&mut self, state: &mut State, id: &str, tokens: Tokens) -> Result<(), CommentaryFailure> {
        let secret = serde_json::to_string(&tokens).map_err(|_| failure("protocol"))?;
        let saved = self
            .store
            .write("chatgpt-session", &Self::key(state, id), &secret)
            .is_ok();
        let account = state.accounts.get_mut(id).ok_or_else(|| failure("loginRequired"))?;
        account.session_only = !saved;
        account.authorized = tokens.authorized();
        if saved {
            self.memory.remove(id);
        } else {
            self.memory.insert(id.to_owned(), tokens);
        }
        Ok(())
    }
    pub fn facts(&mut self, signing_in: bool) -> Result<CommentaryChatGptFacts, CommentaryFailure> {
        self.locked(|this, state| {
            let accounts = state
                .accounts
                .iter()
                .map(|(id, account)| {
                    let tokens = this.tokens(state, id);
                    CommentaryAccount {
                        id: id.clone(),
                        email: account.email.clone(),
                        model: account.model.clone(),
                        effort: account
                            .reasoning_by_model
                            .get(&account.model)
                            .filter(|v| chatgpt::valid_effort(v))
                            .cloned(),
                        signed_in: tokens
                            .as_ref()
                            .is_ok_and(|t| !t.access_token.trim().is_empty() || !t.id_token.trim().is_empty()),
                        authorized: account.authorized,
                        session_only: account.session_only,
                        credentials_unavailable: tokens
                            .as_ref()
                            .is_err_and(|e| e.message_key == "Teacher.chatgpt.error.credentialsUnavailable"),
                    }
                })
                .collect();
            Ok(CommentaryChatGptFacts {
                accounts,
                active: state.active.clone().filter(|id| state.accounts.contains_key(id)),
                signing_in,
                models: this
                    .models
                    .iter()
                    .map(|m| CommentaryModel {
                        slug: m.slug.clone(),
                        display_name: m.display_name.clone(),
                        efforts: m.efforts.clone(),
                        default_effort: (!m.default_effort.is_empty()).then(|| m.default_effort.clone()),
                    })
                    .collect(),
                status_key: this.status_key.clone(),
            })
        })
    }
    pub fn sign_in_context(&mut self, profile: Option<String>) -> Result<SignInContext, CommentaryFailure> {
        self.locked(|_, state| {
            let account = match profile.as_ref() {
                Some(id) => Some(state.accounts.get(id).ok_or_else(|| failure("loginRequired"))?),
                None => None,
            };
            Ok(SignInContext {
                profile: profile.clone(),
                client: account.map(|a| a.client.clone()).unwrap_or_else(|| {
                    state
                        .pending_registration
                        .clone()
                        .unwrap_or_else(|| "dynamic_agent_client".into())
                }),
                host: state.host.clone(),
                subject: account.map(|a| a.subject.clone()),
                revision: state.authorization_revision,
                reconsent: account.is_some_and(|a| !a.authorized),
            })
        })
    }
    pub fn retain_registration(&mut self, client: &str) -> Result<(), CommentaryFailure> {
        self.locked(|this, state| {
            state.pending_registration = Some(client.into());
            this.persist(state)
        })
    }
    pub fn accept(
        &mut self,
        context: &SignInContext,
        client: &str,
        identity: identity::Identity,
        reply: &Value,
    ) -> Result<(), CommentaryFailure> {
        let tokens = Tokens::normalize(reply, None)?;
        self.locked(|this, state| {
            if state.authorization_revision != context.revision {
                return Err(failure("loginRequired"));
            }
            let id = if let Some(id) = &context.profile {
                let account = state.accounts.get(id).ok_or_else(|| failure("identity"))?;
                if account.client != client || account.subject != identity.subject {
                    return Err(failure("identity"));
                }
                id.clone()
            } else {
                state
                    .accounts
                    .iter()
                    .find(|(_, a)| a.client == client && a.subject == identity.subject)
                    .map(|(id, _)| id.clone())
                    .map(Ok)
                    .unwrap_or_else(uuid)?
            };
            let account = state.accounts.entry(id.clone()).or_default();
            account.client = client.into();
            account.subject = identity.subject;
            account.email = identity.email;
            account.signed_out = false;
            this.save_tokens(state, &id, tokens)?;
            if state.pending_registration.as_deref() == Some(client) {
                state.pending_registration = None;
            }
            state.active = Some(id);
            this.persist(state)?;
            this.generation += 1;
            Ok(())
        })
    }
    pub fn select(&mut self, id: &str) -> Result<(), CommentaryFailure> {
        self.locked(|this, state| {
            if !state.accounts.contains_key(id) {
                return Err(failure("loginRequired"));
            }
            state.active = Some(id.into());
            this.persist(state)?;
            this.models.clear();
            this.generation += 1;
            Ok(())
        })
    }
    pub fn save_connection(&mut self, id: &str, model: &str, effort: Option<&str>) -> Result<(), CommentaryFailure> {
        let capability = self
            .models
            .iter()
            .find(|m| m.slug == model)
            .ok_or_else(|| failure("models"))?;
        if effort.is_some_and(|e| !e.is_empty() && !capability.efforts.iter().any(|v| v == e)) {
            return Err(failure("reasoning"));
        }
        self.locked(|this, state| {
            let tokens = this.tokens(state, id)?;
            if !tokens.authorized() {
                return Err(failure("permission"));
            }
            let account = state.accounts.get_mut(id).ok_or_else(|| failure("loginRequired"))?;
            account.model = model.into();
            account.welcomed = true;
            if let Some(effort) = effort.filter(|e| !e.is_empty()) {
                account.reasoning_by_model.insert(model.into(), effort.into());
            } else {
                account.reasoning_by_model.remove(model);
            }
            state.active = Some(id.into());
            this.persist(state)?;
            this.generation += 1;
            Ok(())
        })
    }
    pub fn active(&mut self) -> Result<Option<String>, CommentaryFailure> {
        self.locked(|_, state| Ok(state.active.clone()))
    }
    pub fn access_token(&mut self, id: &str, http: &mut Http) -> Result<String, CommentaryFailure> {
        self.locked(|this, state| {
            let mut tokens = this.tokens(state, id)?;
            if !tokens.authorized() {
                return Err(failure("permission"));
            }
            if tokens.expires_at <= now() + 60 {
                if tokens.refresh_token.trim().is_empty() {
                    return Err(failure("loginRequired"));
                }
                let account = state.accounts.get(id).ok_or_else(|| failure("loginRequired"))?;
                let client = account.client.clone();
                let subject = account.subject.clone();
                let reply = http
                    .form(
                        &format!("{AUTH}/api/accounts/oauth/token"),
                        &[
                            ("grant_type", "refresh_token"),
                            ("client_id", &client),
                            ("refresh_token", &tokens.refresh_token),
                            ("resource", RESOURCE),
                        ],
                    )
                    .map_err(|e| {
                        if e.cancelled {
                            CommentaryFailure::cancelled()
                        } else {
                            failure("network")
                        }
                    })?;
                let reply = match json_reply(reply) {
                    Ok(reply) => reply,
                    Err(error) => {
                        if error.refresh_revoked() {
                            state
                                .accounts
                                .get_mut(id)
                                .ok_or_else(|| failure("loginRequired"))?
                                .signed_out = true;
                            this.memory.remove(id);
                            this.persist(state)?;
                            let _ = this.store.delete("chatgpt-session", &Self::key(state, id));
                            this.generation += 1;
                        }
                        return Err(error.failure());
                    }
                };
                if let Some(token) = reply["id_token"].as_str() {
                    verify_identity(http, token, &client, None, Some(&subject))?;
                }
                tokens = Tokens::normalize(&reply, Some(&tokens))?;
                this.save_tokens(state, id, tokens.clone())?;
                this.persist(state)?;
            }
            if !tokens.authorized() {
                return Err(failure("permission"));
            }
            Ok(tokens.access_token)
        })
    }
    pub fn sign_out(&mut self, id: &str, http: &mut Http) -> Result<bool, CommentaryFailure> {
        self.generation += 1;
        self.locked(|this, state| {
            let account = state.accounts.get(id).ok_or_else(|| failure("loginRequired"))?;
            let tokens = this.tokens(state, id);
            let mut revoked = tokens.as_ref().is_ok_and(|t| t.refresh_token.is_empty());
            if let Ok(tokens) = tokens {
                if !tokens.refresh_token.is_empty() {
                    for attempt in 0..2 {
                        if attempt > 0 {
                            std::thread::sleep(Duration::from_millis(500));
                        }
                        if revoke(http, &account.client, &tokens.refresh_token) {
                            revoked = true;
                            break;
                        }
                    }
                }
            }
            state.authorization_revision += 1;
            state
                .accounts
                .get_mut(id)
                .ok_or_else(|| failure("loginRequired"))?
                .signed_out = true;
            this.memory.remove(id);
            this.persist(state)?;
            if this.store.delete("chatgpt-session", &Self::key(state, id)).is_err() {
                revoked = false;
            }
            this.models.clear();
            Ok(revoked)
        })
    }
    pub fn clear_memory(&mut self) {
        self.memory.clear();
    }
}

fn restrict(path: &Path, directory: bool) -> Result<(), CommentaryFailure> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(
            path,
            std::fs::Permissions::from_mode(if directory { 0o700 } else { 0o600 }),
        )
        .map_err(|_| failure("storage"))?;
    }
    #[cfg(not(unix))]
    {
        let _ = (path, directory);
    }
    Ok(())
}
pub fn json_reply(mut reply: HttpReply) -> Result<Value, chatgpt::ApiError> {
    let mut bytes = Vec::new();
    let limit = if reply.status / 100 == 2 { 2_097_152 } else { 65_536 };
    if reply.body.by_ref().take(limit + 1).read_to_end(&mut bytes).is_err() || bytes.len() as u64 > limit {
        return Err(chatgpt::classify_error(0, &Value::Null, ""));
    }
    let payload = serde_json::from_slice(&bytes).unwrap_or(Value::Null);
    if reply.status / 100 != 2 {
        return Err(chatgpt::classify_error(reply.status, &payload, ""));
    }
    if !payload.is_object() {
        return Err(chatgpt::classify_error(400, &Value::Null, ""));
    }
    Ok(payload)
}
pub fn verify_identity(
    http: &mut Http,
    token: &str,
    client: &str,
    nonce: Option<&str>,
    subject: Option<&str>,
) -> Result<identity::Identity, CommentaryFailure> {
    let keys = http
        .get(&format!("{AUTH}/.well-known/jwks.json"), &[], Duration::from_secs(30))
        .map_err(|_| failure("network"))?;
    identity::verify(
        token,
        &json_reply(keys).map_err(|e| e.failure())?,
        client,
        nonce,
        subject,
        now(),
    )
}
fn revoke(http: &mut Http, client: &str, token: &str) -> bool {
    let attempt = (|| {
        let reply = http
            .get(
                &format!("{AUTH}/.well-known/openid-configuration"),
                &[],
                Duration::from_secs(30),
            )
            .ok()?;
        let discovery = json_reply(reply).ok()?;
        let url = reqwest::Url::parse(discovery["revocation_endpoint"].as_str()?).ok()?;
        if url.scheme() != "https"
            || url.host_str() != Some("auth.openai.com")
            || url.port_or_known_default() != Some(443)
            || !url.username().is_empty()
            || url.password().is_some()
        {
            return None;
        }
        let reply = http
            .form(
                url.as_str(),
                &[
                    ("token", token),
                    ("token_type_hint", "refresh_token"),
                    ("client_id", client),
                ],
            )
            .ok()?;
        Some(reply.status == 200)
    })();
    attempt.unwrap_or(false)
}

pub fn fresh_cancel() -> Arc<AtomicBool> {
    Arc::new(AtomicBool::new(false))
}
pub fn cancelled(cancel: &AtomicBool) -> Result<(), CommentaryFailure> {
    if cancel.load(Ordering::Acquire) {
        Err(CommentaryFailure::cancelled())
    } else {
        Ok(())
    }
}

impl Sessions {
    pub fn refreshed_status(&mut self) -> Result<String, CommentaryFailure> {
        self.locked(|this, state| {
            let Some(id) = state.active.as_ref() else {
                return Ok(String::new());
            };
            let Some(account) = state.accounts.get_mut(id) else {
                return Ok(String::new());
            };
            if account.authorized && !account.welcomed {
                account.welcomed = true;
                this.persist(state)?;
                return Ok("Teacher.chatgpt.welcome".into());
            }
            if !account.model.is_empty() && !this.models.iter().any(|model| model.slug == account.model) {
                return Ok("Teacher.chatgpt.error.modelRemoved".into());
            }
            Ok(String::new())
        })
    }
}
