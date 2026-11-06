use super::CommentaryFailure;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use ts_rs::TS;
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(rename_all = "camelCase")]
pub enum Provider {
    #[default]
    Unselected,
    ChatGpt,
    ApiKey,
}
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct Preferences {
    pub rank_mode: String,
    pub rank_num: i32,
    pub style_index: i32,
    pub density_index: i32,
    pub pace_index: i32,
    pub variation_index: i32,
}
impl Default for Preferences {
    fn default() -> Self {
        Self {
            rank_mode: "k".into(),
            rank_num: 5,
            style_index: 0,
            density_index: 1,
            pace_index: 1,
            variation_index: 1,
        }
    }
}
impl Preferences {
    pub fn normalized(mut self) -> Self {
        self.rank_mode = if self.rank_mode.eq_ignore_ascii_case("d") {
            "d"
        } else {
            "k"
        }
        .into();
        self.rank_num = self.rank_num.clamp(1, if self.rank_mode == "d" { 9 } else { 18 });
        self.style_index = self.style_index.clamp(0, 4);
        self.density_index = self.density_index.clamp(0, 2);
        self.pace_index = self.pace_index.clamp(0, 2);
        self.variation_index = self.variation_index.clamp(0, 2);
        self
    }
}
#[derive(Clone, Debug, Default, PartialEq, Eq, Serialize, Deserialize, TS)]
#[serde(default, rename_all = "camelCase")]
pub struct CommentarySettings {
    pub provider: Provider,
    pub base_url: String,
    pub model: String,
    pub remember_api_key: bool,
    #[serde(flatten)]
    pub preferences: Preferences,
}
pub fn validate_base_url(value: &str) -> Result<String, CommentaryFailure> {
    let value = value.trim_matches(|c: char| c <= '\u{20}');
    if value.is_empty() {
        return Err(CommentaryFailure::keyed(
            "Teacher.settings.enterAddress",
            "Enter the address supplied by your provider.",
        ));
    }
    let value = value.trim_end_matches('/');
    if value.chars().any(|c| c.is_whitespace() || c.is_control()) {
        return Err(CommentaryFailure::new("Base URL is invalid."));
    }
    let Some((scheme, rest)) = value.split_once("://") else {
        return Err(CommentaryFailure::new("Base URL must be an HTTP(S) server URL."));
    };
    let authority = rest.split('/').next().unwrap_or("");
    let bad = || CommentaryFailure::new("Base URL must be an HTTP(S) server URL.");
    if !(scheme.eq_ignore_ascii_case("http") || scheme.eq_ignore_ascii_case("https"))
        || authority.is_empty()
        || authority.contains('@')
        || rest.contains(['?', '#'])
    {
        return Err(bad());
    }
    let (host, port) = if let Some(ip) = authority.strip_prefix('[') {
        let Some((host, suffix)) = ip.split_once(']') else {
            return Err(bad());
        };
        if host.parse::<std::net::Ipv6Addr>().is_err() {
            return Err(bad());
        }
        (
            host,
            suffix
                .strip_prefix(':')
                .or(if suffix.is_empty() { Some("") } else { None })
                .ok_or_else(bad)?,
        )
    } else {
        let (h, p) = authority.split_once(':').unwrap_or((authority, ""));
        if h.is_empty()
            || !h.bytes().all(|b| b.is_ascii_alphanumeric() || b == b'-' || b == b'.')
            || h.starts_with('-')
            || h.ends_with('-')
        {
            return Err(bad());
        }
        (h, p)
    };
    if !port.is_empty() && port.parse::<u16>().is_err() {
        return Err(bad());
    }
    if scheme.eq_ignore_ascii_case("http")
        && !["localhost", "127.0.0.1", "::1", "0:0:0:0:0:0:0:1"]
            .iter()
            .any(|h| host.eq_ignore_ascii_case(h))
    {
        return Err(CommentaryFailure::new(
            "Plain HTTP is allowed only for a provider running on this computer.",
        ));
    }
    // URI rejects malformed percent escapes in paths, even though it preserves them verbatim.
    let bytes = rest.as_bytes();
    for (i, b) in bytes.iter().enumerate() {
        if *b == b'%'
            && (i + 2 >= bytes.len() || !bytes[i + 1].is_ascii_hexdigit() || !bytes[i + 2].is_ascii_hexdigit())
        {
            return Err(CommentaryFailure::new("Base URL is invalid."));
        }
    }
    Ok(value.into())
}
pub fn validate_model(value: &str) -> Result<String, CommentaryFailure> {
    let v = value.trim_matches(|c: char| c <= '\u{20}');
    if v.is_empty() {
        return Err(CommentaryFailure::keyed(
            "Teacher.settings.enterModel",
            "Choose a model from your provider or enter its name.",
        ));
    }
    if v.encode_utf16().count() > 160 || v.chars().any(|c| c <= '\u{1f}' || ('\u{7f}'..='\u{9f}').contains(&c)) {
        return Err(CommentaryFailure::new("Model name is invalid."));
    }
    Ok(v.into())
}
pub fn credential_account(base_url: &str) -> Result<String, CommentaryFailure> {
    let digest = Sha256::digest(validate_base_url(base_url)?.as_bytes());
    Ok(format!(
        "ai-commentary:{}",
        digest[..12].iter().map(|b| format!("{b:02x}")).collect::<String>()
    ))
}
