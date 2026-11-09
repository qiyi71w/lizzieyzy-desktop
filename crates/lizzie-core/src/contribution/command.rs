//! The contribution command line, kept character for character as Java
//! `ContributeEngine`'s constructor builds it (`ContributeEngine.java:171-215`), and the CA
//! certificate recovery variant (`certificateRecoveryCommand`, `:410-419`).

use std::path::{Path, PathBuf};

use crate::settings::ContributionSettings;

/// The default server, appended as the first override value when no config file is used.
pub const DEFAULT_SERVER: &str = "\"serverUrl=https://katagotraining.org/\",";

/// `new ContributeEngine()`'s `engineCommand`. `password` is the training password read by the
/// platform adapter; it is part of the command line like in Java and must be masked at every
/// surface (`sanitize`).
pub fn build(settings: &ContributionSettings, password: Option<&str>) -> String {
    let pure = settings.use_command && settings.use_pure_command;
    let mut command = if settings.use_command {
        let mut command = settings.command.clone();
        if !settings.use_pure_command {
            if !command.contains("-override") {
                command.push_str(" -override-config ");
            }
            if !command.contains(" -config") {
                command.push_str(" \"serverUrl=https://katagotraining.org/\",");
            }
        }
        command
    } else {
        let mut command = format!("{} contribute", settings.engine_path);
        let use_config = !settings.config_path.trim().is_empty();
        if use_config {
            command.push_str(" -config ");
            command.push_str(&settings.config_path);
        }
        command.push_str(" -override-config ");
        if !use_config {
            command.push_str(DEFAULT_SERVER);
        }
        command
    };
    if !pure {
        command.push_str(&format!("\"username={}\",", settings.user_name));
        command.push_str(&format!("\"password={}\",", password.unwrap_or("")));
        command.push_str(&format!("\"maxSimultaneousGames={}\",", settings.batch_games));
        command.push_str(&format!(
            "\"includeOwnership={}\",",
            if settings.show_estimate { "true" } else { "false" }
        ));
        if settings.disable_rating_matches {
            command.push_str("\"maxRatingMatches=0\",");
        }
        command.push_str("\"logGamesAsJson=true\"");
    }
    command
}

/// Written in place of each `password=` value of a saved custom command. The command as typed is
/// kept by the platform adapter (system credential storage), never in `config.json` or the
/// settings projection.
pub const SAVED_PASSWORD: &str = "******";

/// The `(start, end)` byte range of each `password=` value in `command`, read with the quoting of
/// [`crate::engine::gtp::split_command`] (`'…'`, `"…"`, joined runs): the value ends at a `,`
/// (KataGo splits override values there), at the quote closing a run it started in, or at
/// whitespace outside quotes.
fn password_values(command: &str) -> Vec<(usize, usize)> {
    #[derive(Clone, Copy, PartialEq)]
    enum Quote {
        None,
        Single,
        Double,
    }
    let next = |quote: Quote, c: char| match (quote, c) {
        (Quote::None, '\'') => Quote::Single,
        (Quote::None, '"') => Quote::Double,
        (Quote::Single, '\'') | (Quote::Double, '"') => Quote::None,
        (quote, _) => quote,
    };
    const KEY: &str = "password=";
    let mut values = Vec::new();
    let mut quote = Quote::None;
    let mut at = 0;
    while at < command.len() {
        if command[at..].starts_with(KEY) {
            let start = at + KEY.len();
            let outer = quote;
            let mut end = command.len();
            for (offset, c) in command[start..].char_indices() {
                let after = next(quote, c);
                let closes_outer = outer != Quote::None && quote == outer && after == Quote::None;
                if c == ',' || closes_outer || (quote == Quote::None && c.is_whitespace()) {
                    end = start + offset;
                    break;
                }
                quote = after;
            }
            values.push((start, end));
            at = end;
            continue;
        }
        let Some(c) = command[at..].chars().next() else { break };
        quote = next(quote, c);
        at += c.len_utf8();
    }
    values
}

/// `command` as it is saved and shown: each non-empty `password=` value replaced by
/// [`SAVED_PASSWORD`].
pub fn conceal_passwords(command: &str) -> String {
    let mut saved = String::with_capacity(command.len());
    let mut last = 0;
    for (start, end) in password_values(command) {
        if start == end {
            continue;
        }
        saved.push_str(&command[last..start]);
        saved.push_str(SAVED_PASSWORD);
        last = end;
    }
    saved.push_str(&command[last..]);
    saved
}

/// The custom command to run: the one the adapter kept as typed (`kept`) when it is what the saved
/// `command` conceals, otherwise the saved command itself.
pub fn command_to_run(command: &str, kept: Option<&str>) -> String {
    match kept {
        Some(kept) if conceal_passwords(kept) == command => kept.to_string(),
        _ => command.to_string(),
    }
}

/// `ContributeEngine.certificateRecoveryCommand`: append KataGo's bundled CA file, resolved
/// beside the engine executable. Remote and pure commands are left as they are.
pub fn certificate_recovery(command: &str, executable: &str, remote: bool, pure: bool) -> String {
    if remote || pure {
        return command.to_string();
    }
    let absolute = std::path::absolute(Path::new(executable)).unwrap_or_else(|_| PathBuf::from(executable));
    let directory = absolute.parent().map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    format!(
        "{command} -cacerts \"{}\"",
        directory.join("cacert.pem").display()
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    fn settings() -> ContributionSettings {
        ContributionSettings {
            engine_path: "/opt/katago".into(),
            user_name: "alice".into(),
            batch_games: "6".into(),
            ..ContributionSettings::default()
        }
    }

    #[test]
    fn a_plain_engine_path_gets_the_server_and_the_overrides() {
        let command = build(&settings(), Some("secret"));
        assert_eq!(
            command,
            "/opt/katago contribute -override-config \"serverUrl=https://katagotraining.org/\",\"username=alice\",\"password=secret\",\"maxSimultaneousGames=6\",\"includeOwnership=false\",\"logGamesAsJson=true\""
        );
    }

    #[test]
    fn a_config_file_replaces_the_server_override() {
        let settings = ContributionSettings {
            config_path: " /tmp/gtp.cfg ".into(),
            disable_rating_matches: true,
            show_estimate: true,
            ..settings()
        };
        let command = build(&settings, None);
        assert_eq!(
            command,
            "/opt/katago contribute -config  /tmp/gtp.cfg  -override-config \"username=alice\",\"password=\",\"maxSimultaneousGames=6\",\"includeOwnership=true\",\"maxRatingMatches=0\",\"logGamesAsJson=true\""
        );
    }

    #[test]
    fn a_pure_custom_command_is_left_alone() {
        let settings = ContributionSettings {
            use_command: true,
            use_pure_command: true,
            command: "my-katago contribute".into(),
            ..settings()
        };
        assert_eq!(build(&settings, Some("secret")), "my-katago contribute");
    }

    #[test]
    fn a_custom_command_gets_the_missing_override_and_config() {
        let settings = ContributionSettings {
            use_command: true,
            command: "my-katago contribute".into(),
            ..settings()
        };
        let command = build(&settings, Some("secret"));
        // Java appends " -override-config " and then " \"serverUrl=…\",": two spaces.
        assert!(command.starts_with(
            "my-katago contribute -override-config  \"serverUrl=https://katagotraining.org/\",\"username="
        ));
    }

    #[test]
    fn certificate_recovery_resolves_beside_the_engine() {
        assert_eq!(
            certificate_recovery("/opt/katago contribute", "/opt/katago", false, false),
            "/opt/katago contribute -cacerts \"/opt/cacert.pem\""
        );
        assert_eq!(
            certificate_recovery("/opt/katago contribute", "/opt/katago", true, false),
            "/opt/katago contribute"
        );
    }

    #[test]
    fn every_password_written_in_a_command_is_concealed_whole() {
        let cases = [
            (
                "k -override-config \"username=a, password=x y,z\",\"password=p2\" password=p3 -x",
                "k -override-config \"username=a, password=******,z\",\"password=******\" password=****** -x",
            ),
            ("k -override-config username=bob,password=\"TOP SECRET\"", "k -override-config username=bob,password=******"),
            ("k -override-config 'username=alice,password=TOP SECRET'", "k -override-config 'username=alice,password=******'"),
            ("k -override-config \"username=a,password=\"", "k -override-config \"username=a,password=\""),
        ];
        for (typed, saved) in cases {
            assert_eq!(conceal_passwords(typed), saved);
            assert_eq!(command_to_run(saved, Some(typed)), typed);
        }
        // A saved command edited since the typed one was kept runs as saved.
        assert_eq!(command_to_run("k password=******", Some("k password=old -x")), "k password=******");
    }
}
