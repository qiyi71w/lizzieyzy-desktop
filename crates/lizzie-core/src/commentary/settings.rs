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
    // Java `URI`: a port is decimal digits only (`str::parse` would also take a leading `+`).
    if !port.is_empty() && (!port.bytes().all(|b| b.is_ascii_digit()) || port.parse::<u16>().is_err()) {
        return Err(bad());
    }
    if scheme.eq_ignore_ascii_case("http") && !allows_plain_http(host) {
        return Err(CommentaryFailure::new(
            "Plain HTTP requires localhost or a literal private network IP address.",
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
/// Java `TeacherServiceHost.allowsPlainHttp`: the hosts an `http://` address may name. `localhost`,
/// `127.0.0.1`, a private IPv4 address (10/8, 172.16/12, 192.168/16) in dotted decimal, or the
/// IPv6 loopback or a unique local address (fc00::/7). Only literal addresses count: a name is
/// never looked up, and other spellings of an address (a single number, octal or hexadecimal
/// parts, IPv4 inside IPv6, a scope) are refused, so what is checked is what is connected to.
fn allows_plain_http(host: &str) -> bool {
    if host.eq_ignore_ascii_case("localhost") {
        return true;
    }
    if host.contains(':') {
        if !host.bytes().all(|b| b.is_ascii_hexdigit() || b == b':') {
            return false;
        }
        return host
            .parse::<std::net::Ipv6Addr>()
            .is_ok_and(|ip| ip.to_ipv4_mapped().is_none() && (ip.is_loopback() || ip.octets()[0] & 0xfe == 0xfc));
    }
    let mut octets = [0u8; 4];
    let mut parts = host.split('.');
    for octet in &mut octets {
        let Some(part) = parts.next() else { return false };
        // Java `0|[1-9][0-9]{0,2}`: plain decimal without a leading zero (which some parsers read
        // as octal) or a sign.
        let decimal = part.bytes().all(|b| b.is_ascii_digit()) && !(part.len() > 1 && part.starts_with('0'));
        let Some(value) = part.parse::<u8>().ok().filter(|_| decimal) else {
            return false;
        };
        *octet = value;
    }
    if parts.next().is_some() {
        return false;
    }
    match octets {
        [127, 0, 0, 1] | [10, ..] | [192, 168, ..] => true,
        [172, second, ..] => (16..=31).contains(&second),
        _ => false,
    }
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
