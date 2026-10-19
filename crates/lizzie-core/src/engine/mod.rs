//! Engines (引擎运行): configured engines, the GTP protocol, the running instances and the
//! real process adapter.
//!
//! - [`profile`]: committed engine settings.
//! - [`gtp`]: command lines, response framing, analysis lines, vertices.
//! - [`rules`]: KataGo rules values.
//! - [`runtime`]: instances, command confirmation, request ownership, foreground and exit.
//! - [`process`]: the OS process launcher.

pub mod gtp;
pub mod process;
pub mod profile;
pub mod rules;
pub mod runtime;

/// Credential parameter names masked in logs and traces (`PersistenceSanitizer`). Local paths
/// stay readable so launch failures remain diagnosable.
fn is_secret_key(key: &str) -> bool {
    let k = key.trim_start_matches('-').to_ascii_lowercase().replace('_', "-");
    let k = k.as_str();
    matches!(
        k,
        "pw" | "password"
            | "passwd"
            | "token"
            | "secret"
            | "connectpassword"
            | "zhizi-account-token"
            | "zz-socketio-token"
    ) || [
        "machinekey",
        "machine-key",
        "apikey",
        "api-key",
        "x-apikey",
        "x-api-key",
        "xapikey",
        "xapi-key",
    ]
    .contains(&k)
        || [
            "accesstoken",
            "access-token",
            "x-accesstoken",
            "x-access-token",
            "xaccesstoken",
            "xaccess-token",
        ]
        .contains(&k)
        || ["refreshtoken", "refresh-token", "clientsecret", "client-secret"].contains(&k)
}

/// Masks credential values in a command line or protocol line: `--password x`, `password=x`
/// and URL query parameters become `<redacted>`.
pub fn redact(text: &str) -> String {
    let tokens: Vec<&str> = text.split(' ').collect();
    let mut out: Vec<String> = Vec::with_capacity(tokens.len());
    let mut mask_next = false;
    for t in tokens {
        if mask_next && !t.is_empty() {
            out.push("<redacted>".to_string());
            mask_next = false;
            continue;
        }
        let mut changed = String::new();
        let mut rest = t;
        let mut any = false;
        // key=value pieces separated by '?' or '&' (URL queries) or a bare key=value.
        while let Some(eq) = rest.find('=') {
            let key_start = rest[..eq].rfind(['?', '&']).map_or(0, |i| i + 1);
            let value_end = rest[eq + 1..].find('&').map_or(rest.len(), |i| eq + 1 + i);
            changed.push_str(&rest[..eq + 1]);
            if is_secret_key(&rest[key_start..eq]) {
                changed.push_str("<redacted>");
                any = true;
            } else {
                changed.push_str(&rest[eq + 1..value_end]);
            }
            rest = &rest[value_end..];
        }
        changed.push_str(rest);
        if !any && t.starts_with('-') && is_secret_key(t) {
            mask_next = true;
        }
        out.push(changed);
    }
    out.join(" ")
}

#[cfg(test)]
mod tests {
    #[test]
    fn credentials_are_masked_and_paths_kept() {
        assert_eq!(
            super::redact("/opt/kata/katago gtp -model /m.bin.gz --password hunter2 https://h/x?token=abc&n=1 api_key=z"),
            "/opt/kata/katago gtp -model /m.bin.gz --password <redacted> https://h/x?token=<redacted>&n=1 api_key=<redacted>"
        );
    }
}
