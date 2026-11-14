//! 检查更新 (Java `featurecat.lizzie.update`): the saved update preferences, which installation
//! this client is, and the discovery of a newer release on the project's GitHub Releases.
//!
//! A release is described by one update manifest, the static manifest of the Tauri updater with
//! a few additions: `prerelease` (the release's channel), `notesUrl` (查看 Release) and, per
//! platform entry, the package `size` and optional download `mirrors`. Platform entries are keyed
//! `{os}-{arch}-{form}` (for example `windows-x86_64-nsis`), so the current installation form is
//! chosen exactly and a portable or package-managed client is never offered an installer.
//! Every package carries the Tauri updater signature, which binds the package bytes and its
//! version to the project's update key; it is checked by the platform adapter before a package
//! counts as prepared and again before it is installed.
//!
//! This module does no I/O of its own: manifests are read through a caller-owned
//! [`crate::resources::Fetch`].

pub mod portable;

use crate::resources::Fetch;
use serde::{Deserialize, Serialize};
use std::collections::BTreeMap;
use std::io::Read;
use ts_rs::TS;

/// The update manifest's file name, a release asset of every release.
pub const MANIFEST_NAME: &str = "lizzieyzy-tauri-update.json";
/// 正式 on GitHub: the manifest of the latest release that is not a pre-release.
pub const GITHUB_STABLE_MANIFEST: &str =
    "https://github.com/qiyi71w/lizzieyzy-desktop/releases/latest/download/lizzieyzy-tauri-update.json";
/// 测试: the manifest of the newest test release, re-attached to the `channel-beta` release
/// whenever a test release is published (Java's `channel-beta` pointer).
pub const GITHUB_BETA_MANIFEST: &str =
    "https://github.com/qiyi71w/lizzieyzy-desktop/releases/download/channel-beta/lizzieyzy-tauri-update.json";
/// 官网's own manifest mirrors, tried before GitHub. The project has no site of its own yet, so
/// 官网 reads GitHub Releases.
pub const OFFICIAL_STABLE_MANIFESTS: &[&str] = &[];
/// Java `UpdateManifestClient.MAX_ENVELOPE_BYTES`.
const MAX_MANIFEST_BYTES: u64 = 1024 * 1024;

/// 更新通道 (`update-channel`): 正式 or 测试. Switching it changes no installed file.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum UpdateChannel {
    #[default]
    Stable,
    Beta,
}

/// 更新源 (`update-source`) of the 正式 channel; 测试 always reads GitHub.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum UpdateSource {
    #[default]
    Official,
    Github,
}

/// The saved update preferences (Java `ui.auto-check-update`, `ui.auto-download-update`,
/// `ui.update-channel`, `ui.update-source`; all off, 正式, 官网 by default). The automatic ones
/// take effect at the next start; turning one off takes effect at once.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct UpdateSettings {
    /// 启动时自动检查更新.
    pub auto_check: bool,
    /// 自动下载更新: only with 启动时自动检查更新 (`UpdatePreferences.autoDownload`).
    pub auto_download: bool,
    pub channel: UpdateChannel,
    pub source: UpdateSource,
}

impl UpdateSettings {
    /// 自动下载更新 counts only while 启动时自动检查更新 is on.
    pub fn normalized(mut self) -> Self {
        self.auto_download &= self.auto_check;
        self
    }
}

/// How this client was installed, which decides the package an update needs and how it is
/// handed over.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum InstallForm {
    /// Windows NSIS installation.
    Nsis,
    /// macOS application bundle.
    App,
    /// Linux AppImage.
    AppImage,
    /// Windows portable zip.
    PortableZip,
    /// Ubuntu deb package.
    Deb,
    /// A form the project does not distribute (MSI, RPM).
    Other,
    /// A development or unpackaged build: no update applies.
    Unpackaged,
}

impl InstallForm {
    /// The `{form}` part of the manifest key.
    fn key_name(self) -> Option<&'static str> {
        match self {
            InstallForm::Nsis => Some("nsis"),
            InstallForm::App => Some("app"),
            InstallForm::AppImage => Some("appimage"),
            InstallForm::PortableZip => Some("zip"),
            InstallForm::Deb => Some("deb"),
            InstallForm::Other | InstallForm::Unpackaged => None,
        }
    }

    /// Forms whose installation the client hands over itself: the Tauri updater's routes and the
    /// portable zip's own file replacement ([`portable`]).
    pub fn hands_over(self) -> bool {
        matches!(
            self,
            InstallForm::Nsis | InstallForm::App | InstallForm::AppImage | InstallForm::PortableZip
        )
    }
}

/// The running client: its version, platform, architecture and installation form, and whether
/// this build knows the project's update key.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct InstalledApp {
    pub version: semver::Version,
    /// `windows`, `darwin` or `linux`.
    pub os: String,
    /// `x86_64` or `aarch64`.
    pub arch: String,
    pub form: InstallForm,
    pub signing_configured: bool,
}

impl InstalledApp {
    /// The manifest entry this installation takes, `{os}-{arch}-{form}`.
    pub fn asset_key(&self) -> Option<String> {
        self.form
            .key_name()
            .map(|form| format!("{}-{}-{form}", self.os, self.arch))
    }
}

/// One platform entry of a release.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ReleaseAsset {
    pub url: String,
    pub mirrors: Vec<String>,
    /// The Tauri updater signature (the `.sig` file's text).
    pub signature: String,
    pub size: u64,
}

impl ReleaseAsset {
    /// The file name the package is kept under while it is prepared.
    pub fn file_name(&self) -> String {
        let name = self
            .url
            .split(['?', '#'])
            .next()
            .unwrap_or("")
            .rsplit('/')
            .next()
            .unwrap_or("");
        let safe = !name.is_empty()
            && name.chars().next().is_some_and(|c| c.is_ascii_alphanumeric())
            && name
                .chars()
                .all(|c| c.is_ascii_alphanumeric() || matches!(c, '.' | '_' | '-'))
            && !name.ends_with(".part");
        if safe {
            name.to_string()
        } else {
            "update-package".into()
        }
    }
}

/// A parsed update manifest.
#[derive(Clone, Debug, PartialEq)]
pub struct Release {
    pub version: semver::Version,
    pub prerelease: bool,
    pub notes_url: Option<String>,
    pub platforms: BTreeMap<String, ReleaseAsset>,
    /// The manifest as received: the Tauri updater reads its own fields from it at installation.
    pub raw: serde_json::Value,
}

fn http_url(value: &str) -> bool {
    url::Url::parse(value)
        .is_ok_and(|u| matches!(u.scheme(), "https" | "http") && u.host_str().is_some_and(|h| !h.is_empty()))
}

/// Reads an update manifest. Unknown fields are ignored; a malformed entry rejects the whole
/// manifest, as Java's `UpdateManifest.parse` does.
pub fn parse_release(text: &str) -> Result<Release, String> {
    let raw: serde_json::Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let object = raw.as_object().ok_or("the manifest is not an object")?;
    let version = object
        .get("version")
        .and_then(|v| v.as_str())
        .ok_or("the manifest has no version")?;
    let version = semver::Version::parse(version.trim().trim_start_matches('v')).map_err(|e| e.to_string())?;
    let prerelease = match object.get("prerelease") {
        None | Some(serde_json::Value::Null) => false,
        Some(v) => v.as_bool().ok_or("prerelease is not a boolean")?,
    };
    let notes_url = match object.get("notesUrl") {
        None | Some(serde_json::Value::Null) => None,
        Some(v) => {
            let url = v
                .as_str()
                .filter(|u| http_url(u))
                .ok_or("notesUrl is not a web address")?;
            Some(url.to_string())
        }
    };
    let entries = object
        .get("platforms")
        .and_then(|v| v.as_object())
        .ok_or("the manifest has no platforms")?;
    let mut platforms = BTreeMap::new();
    for (key, entry) in entries {
        let text = |name: &str| entry.get(name).and_then(|v| v.as_str()).map(str::trim).unwrap_or("");
        let url = text("url");
        if !http_url(url) {
            return Err(format!("{key} has no valid url"));
        }
        let signature = text("signature");
        if signature.is_empty() {
            return Err(format!("{key} has no signature"));
        }
        let size = entry
            .get("size")
            .and_then(|v| v.as_u64())
            .filter(|n| *n > 0)
            .ok_or(format!("{key} has no size"))?;
        let mut mirrors = Vec::new();
        if let Some(list) = entry.get("mirrors") {
            for mirror in list.as_array().ok_or(format!("{key} mirrors is not a list"))? {
                let mirror = mirror.as_str().map(str::trim).unwrap_or("");
                if !http_url(mirror) {
                    return Err(format!("{key} has an invalid mirror"));
                }
                mirrors.push(mirror.to_string());
            }
        }
        platforms.insert(
            key.clone(),
            ReleaseAsset {
                url: url.into(),
                mirrors,
                signature: signature.into(),
                size,
            },
        );
    }
    if platforms.is_empty() {
        return Err("the manifest has no platforms".into());
    }
    Ok(Release {
        version,
        prerelease,
        notes_url,
        platforms,
        raw,
    })
}

/// What one check found (Java `UpdateCheckResult.Reason`).
#[derive(Clone, Debug, PartialEq)]
pub enum CheckOutcome {
    /// A development or unpackaged build.
    UnavailableBuild,
    /// This build carries no update key, so no release could be trusted.
    SigningNotConfigured,
    /// The installation form updates through another route (not this client's handover).
    UnsupportedForm,
    NoUpdate,
    /// A newer release has no package for this installation.
    NoPackage,
    Offer(Box<Offer>),
    Failed(FailureKind),
}

/// Java `UpdateCheckResult.FailureKind`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
pub enum FailureKind {
    /// No manifest could be read.
    Fetch,
    /// A 测试-channel candidate was unreadable or announced the wrong channel.
    InvalidCandidate,
    Unexpected,
}

/// A newer release with a package for this installation.
#[derive(Clone, Debug, PartialEq)]
pub struct Offer {
    pub release: Release,
    /// The manifest entry chosen, `{os}-{arch}-{form}`.
    pub key: String,
    pub asset: ReleaseAsset,
}

/// The result of one check, with the 测试 channel's incomplete-candidate warnings.
#[derive(Clone, Debug, PartialEq)]
pub struct CheckResult {
    pub outcome: CheckOutcome,
    pub stable_failure: Option<FailureKind>,
    pub beta_failure: Option<FailureKind>,
}

impl CheckResult {
    fn of(outcome: CheckOutcome) -> Self {
        CheckResult {
            outcome,
            stable_failure: None,
            beta_failure: None,
        }
    }
}

/// 更新检查选择快照: the channel and source a check uses, taken when it starts.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Selection {
    pub channel: UpdateChannel,
    pub source: UpdateSource,
}

impl Selection {
    pub fn of(settings: &UpdateSettings) -> Self {
        Selection {
            channel: settings.channel,
            source: settings.source,
        }
    }

    /// The manifests the 正式 channel reads, in order: 官网's mirrors first unless GitHub is
    /// chosen, then GitHub.
    pub fn stable_manifests(&self) -> Vec<String> {
        let mut urls: Vec<String> = Vec::new();
        if self.source == UpdateSource::Official {
            urls.extend(OFFICIAL_STABLE_MANIFESTS.iter().map(|u| u.to_string()));
        }
        urls.push(GITHUB_STABLE_MANIFEST.into());
        urls
    }
}

fn fetch_text(fetch: &dyn Fetch, url: &str) -> Result<String, String> {
    let response = fetch.get(url, None).map_err(|e| e.to_string())?;
    if !(200..300).contains(&response.status) {
        return Err(format!("HTTP {}", response.status));
    }
    if response.total_length.is_some_and(|n| n > MAX_MANIFEST_BYTES) {
        return Err("the manifest is unexpectedly large".into());
    }
    let mut body = Vec::new();
    response
        .body
        .take(MAX_MANIFEST_BYTES + 1)
        .read_to_end(&mut body)
        .map_err(|e| e.to_string())?;
    if body.len() as u64 > MAX_MANIFEST_BYTES {
        return Err("the manifest is unexpectedly large".into());
    }
    String::from_utf8(body).map_err(|e| e.to_string())
}

/// Java `UpdateDiscovery.discover`: one check for `selection`, run off the state lock.
pub fn discover(installed: &InstalledApp, selection: Selection, fetch: &dyn Fetch) -> CheckResult {
    if installed.form == InstallForm::Unpackaged {
        return CheckResult::of(CheckOutcome::UnavailableBuild);
    }
    if !installed.signing_configured {
        return CheckResult::of(CheckOutcome::SigningNotConfigured);
    }
    if !installed.form.hands_over() {
        return CheckResult::of(CheckOutcome::UnsupportedForm);
    }
    if selection.channel == UpdateChannel::Beta {
        let stable = candidate(fetch, GITHUB_STABLE_MANIFEST, false);
        let beta = candidate(fetch, GITHUB_BETA_MANIFEST, true);
        let (stable, stable_failure) = split(stable);
        let (beta, beta_failure) = split(beta);
        let winner = match (stable, beta) {
            (Some(s), Some(b)) => Some(if b.version > s.version { b } else { s }),
            (s, b) => s.or(b),
        };
        let outcome = match winner {
            Some(release) => plan(installed, release),
            None => CheckOutcome::Failed(FailureKind::Fetch),
        };
        return CheckResult {
            outcome,
            stable_failure,
            beta_failure,
        };
    }
    for url in selection.stable_manifests() {
        if let Ok(release) = fetch_text(fetch, &url).and_then(|text| parse_release(&text)) {
            // The 正式 channel never offers a test release.
            if release.prerelease {
                return CheckResult::of(CheckOutcome::NoUpdate);
            }
            return CheckResult::of(plan(installed, release));
        }
    }
    CheckResult::of(CheckOutcome::Failed(FailureKind::Fetch))
}

fn split(r: Result<Release, FailureKind>) -> (Option<Release>, Option<FailureKind>) {
    match r {
        Ok(release) => (Some(release), None),
        Err(kind) => (None, Some(kind)),
    }
}

/// A 测试-channel candidate: readable, and announcing the channel it was published for.
fn candidate(fetch: &dyn Fetch, url: &str, beta: bool) -> Result<Release, FailureKind> {
    let text = fetch_text(fetch, url).map_err(|_| FailureKind::Fetch)?;
    let release = parse_release(&text).map_err(|_| FailureKind::InvalidCandidate)?;
    if release.prerelease != beta {
        return Err(FailureKind::InvalidCandidate);
    }
    Ok(release)
}

fn plan(installed: &InstalledApp, release: Release) -> CheckOutcome {
    if release.version <= installed.version {
        return CheckOutcome::NoUpdate;
    }
    let Some(key) = installed.asset_key() else {
        return CheckOutcome::UnsupportedForm;
    };
    match release.platforms.get(&key).cloned() {
        Some(asset) => CheckOutcome::Offer(Box::new(Offer { release, key, asset })),
        None => CheckOutcome::NoPackage,
    }
}
