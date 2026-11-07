//! Java `RemoteComputeConfig` mapping. Preview keeps only availability, never account secrets.

use serde::Serialize;
use ts_rs::TS;

use super::*;
use crate::cloud::{Catalog, CloudSettings, Provider};
use crate::engine::cloud::{CloudLaunch, CloudProvider};

pub(super) const PREFIX: &str = "leelaz.remote-compute.";
const ZHIZI: &str = "remote-compute://zhizi";
const CUSTOM: &str = "remote-compute://custom-websocket";

/// Independent token and password choices. Availability is not permission to read either secret.
#[derive(Clone, Debug, PartialEq, Serialize, TS)]
#[serde(rename_all = "camelCase")]
pub struct CloudImportCredentials {
    pub identifier: String,
    pub token: bool,
    pub password: bool,
}

fn settings_object(root: &Map<String, Value>) -> Option<&Map<String, Value>> {
    root.get("leelaz")?.get("remote-compute")?.as_object()
}

pub(super) fn credentials(root: &Map<String, Value>) -> Option<CloudImportCredentials> {
    let json = settings_object(root)?;
    let present = |key| json.get(key).and_then(Value::as_str).is_some_and(|s| !s.trim().is_empty());
    let identifier = json.get("zhizi-identifier").filter(|v| !v.is_null()).map(shown).unwrap_or_default();
    // Native stores need not offer metadata-only lookup. Java's remember preference offers a
    // possible copy choice; only the later, explicitly consented read establishes presence.
    let remembered = |key| !identifier.trim().is_empty() && json.get(key).and_then(as_bool).unwrap_or(false);
    Some(CloudImportCredentials {
        token: present("zhizi-account-token") || remembered("remember-zhizi-token"),
        password: present("zhizi-password-v1") || remembered("remember-zhizi-password"),
        identifier,
    })
}

/// Read one legacy secret only after the caller independently obtains consent for that kind.
/// Native-store credentials are deliberately not read here. Java files are never changed.
pub fn read_java_cloud_secret(directory: &Path, password: bool) -> Option<String> {
    use base64::Engine;
    let Value::Object(root) = read_json(&directory.join(CONFIG_FILE)).ok()? else { return None; };
    let json = settings_object(&root)?;
    let key = if password { "zhizi-password-v1" } else { "zhizi-account-token" };
    let value = json.get(key)?.as_str()?;
    let secret = if password {
        // Java Base64.getDecoder (not the MIME decoder), UTF-8 replacement semantics.
        let encoded = value.trim();
        let config = base64::engine::general_purpose::GeneralPurposeConfig::new()
            .with_decode_allow_trailing_bits(true);
        let config = if encoded.contains('=') { config } else {
            config.with_decode_padding_mode(base64::engine::DecodePaddingMode::RequireNone)
        };
        let decoder = base64::engine::general_purpose::GeneralPurpose::new(&base64::alphabet::STANDARD, config);
        String::from_utf8_lossy(&decoder.decode(encoded).ok()?).into_owned()
    } else { value.to_owned() };
    (!secret.trim().is_empty()).then_some(secret)
}

impl Candidate {
    /// Add one kind discovered in Java's native credential store; it never authorizes copying.
    pub fn set_cloud_credential_available(&mut self, password: bool) {
        let Some(settings) = &self.mapped.cloud else { return; };
        let choices = self.cloud_credentials.get_or_insert_with(|| CloudImportCredentials {
            identifier: settings.identifier.clone(), token: false, password: false,
        });
        if password { choices.password = true; } else { choices.token = true; }
        let key = if password { "credential:cloud-password" } else { "credential:cloud-token" };
        if self.items.iter().any(|item| item.key == key) { return; }
        self.items.push(ImportItemView {
            key: key.into(), status: ImportStatus::NotMigrated,
            reason: Some(ImportReason::CredentialNotConfirmed), target: Some(ImportTarget::Cloud),
            value: None, detail: None, credential: true,
        });
    }
}

impl Reader<'_> {
    pub(super) fn cloud(&mut self, leelaz: &Map<String, Value>, handled: &mut Handled) {
        let Some(raw) = leelaz.get("remote-compute") else { return; };
        handled.push(("leelaz", "remote-compute"));
        let Some(json) = raw.as_object() else {
            self.invalid("leelaz.remote-compute".into(), ImportTarget::Cloud, raw);
            return;
        };
        let mut settings = CloudSettings::default();
        let text = |key: &str| json.get(key).filter(|v| !v.is_null()).map(shown).unwrap_or_default();
        settings.provider = match text("provider").as_str() {
            "zhizi" => Provider::Zhizi,
            "custom" => Provider::Custom,
            _ => Provider::Local,
        };
        settings.identifier = text("zhizi-identifier");
        if let Some(args) = json.get("zhizi-args").filter(|v| !v.is_null()) { settings.args = shown(args); }
        settings.custom_url = normalize_url(&text("custom-remote-code"));
        settings.catalog = json.get("zhizi-engine-catalog")
            .and_then(|value| Catalog::from_java(value).ok()).unwrap_or_default();
        settings.catalog_updated_at = json.get("zhizi-catalog-updated-at")
            .and_then(as_i64).unwrap_or(0).max(0) as u64;
        // Remember flags cannot be imported before the shell stores the separately chosen secret.
        for (key, value) in json {
            let name = format!("{PREFIX}{key}");
            match key.as_str() {
                "zhizi-account-token" | "zhizi-password-v1" | "remember-zhizi-token"
                | "remember-zhizi-password" | "zhizi-credential-backend" => self.not_migrated(name, value),
                "provider" | "zhizi-identifier" | "zhizi-args" | "custom-remote-code"
                | "zhizi-engine-catalog" | "zhizi-catalog-updated-at" => {
                    // Catalog and URL text are intentionally not echoed, even for malformed data.
                    let item = self.item(name, ImportStatus::Imported);
                    item.target = Some(ImportTarget::Cloud);
                    item.credential = (key == "custom-remote-code" && url_has_credentials(&settings.custom_url))
                        || (key == "zhizi-args" && sanitize(&settings.args) != settings.args);
                }
                _ => self.not_migrated(name, value),
            }
        }
        self.mapped.cloud = Some(settings);
    }
}

pub(super) fn launch(command: &str, settings: Option<&CloudSettings>) -> Option<CloudLaunch> {
    let provider = if command.trim().starts_with(ZHIZI) { CloudProvider::Zhizi }
        else if command.trim().starts_with(CUSTOM) { CloudProvider::Custom }
        else { return None; };
    let defaults;
    let settings = match settings {
        Some(settings) => settings,
        None => { defaults = CloudSettings::default(); &defaults }
    };
    Some(CloudLaunch { provider, identifier: settings.identifier.clone(), args: settings.args.clone(),
        url: if provider == CloudProvider::Custom { settings.custom_url.clone() } else { String::new() } })
}

pub(super) fn marker(launch: &CloudLaunch) -> &'static str {
    match launch.provider { CloudProvider::Zhizi => ZHIZI, CloudProvider::Custom => CUSTOM }
}

pub(super) fn url_has_credentials(value: &str) -> bool {
    if sanitize(value) != value { return true; }
    // Custom providers use arbitrary query names for access codes. Do not depend on a fixed
    // credential-name list, and fail closed for a malformed nonempty URL.
    match url::Url::parse(value) {
        Ok(url) => !url.username().is_empty() || url.password().is_some()
            || url.query().is_some() || url.fragment().is_some(),
        Err(_) => !value.is_empty(),
    }
}

impl Mapped {
    pub(super) fn cloud_after(&self, copy_credentials: &[u32]) -> Option<CloudSettings> {
        let mut settings = self.cloud.clone()?;
        if url_has_credentials(&settings.custom_url) {
            // The top-level setting and every custom profile refer to the same Java URL. It may
            // enter settings only when at least one affected engine was explicitly confirmed.
            let approved = self.engines.iter().flatten().any(|engine| {
                engine.profile.cloud.as_ref().is_some_and(|launch| launch.provider == CloudProvider::Custom)
                    && copy_credentials.contains(&(engine.java_index as u32))
            });
            if !approved { settings.custom_url.clear(); }
        }
        if sanitize(&settings.args) != settings.args {
            let approved = self.engines.iter().flatten().any(|engine| {
                engine.profile.cloud.is_some() && copy_credentials.contains(&(engine.java_index as u32))
            });
            if !approved { settings.args = crate::cloud::VIP_ARGS.into(); }
        }
        Some(settings)
    }
}

/// `normalizeCustomWebSocketUrl`: pasted links, fullwidth ASCII, quotes and the Java IME typo.
fn normalize_url(value: &str) -> String {
    let mut text: String = value.trim().chars().map(|ch| match ch {
        '\u{3000}' => ' ', '\u{ff01}'..='\u{ff5e}' => char::from_u32(ch as u32 - 0xfee0).unwrap(),
        _ => ch,
    }).collect();
    if (text.starts_with('"') && text.ends_with('"')) || (text.starts_with('\'') && text.ends_with('\'')) {
        if text.len() >= 2 { text = text[1..text.len() - 1].trim().into(); }
    }
    if let Some(rest) = text.strip_prefix("完善://") { text = format!("ws://{rest}"); }
    let lower = text.to_ascii_lowercase();
    if let Some(index) = lower.find("ws://").into_iter().chain(lower.find("wss://")).min() {
        text = text[index..].split(|ch: char| ch.is_whitespace() || matches!(ch, '"' | '\'' | '<' | '>'))
            .next().unwrap_or("").into();
    }
    text.trim().into()
}
