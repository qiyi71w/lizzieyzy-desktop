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

/// Written in place of each `password=` value of a saved custom command: the value itself lives in
/// the system credential storage, never in `config.json` or the settings projection.
pub const SAVED_PASSWORD: &str = "******";

/// The `(start, end)` byte range of each `password=` value in `command`: up to the next `,`, or
/// the closing quote inside a quoted run (KataGo trims and splits override values at commas), or
/// whitespace outside quotes.
fn password_values(command: &str) -> Vec<(usize, usize)> {
    const KEY: &str = "password=";
    let mut values = Vec::new();
    let mut quoted = false;
    let mut at = 0;
    while at < command.len() {
        if command[at..].starts_with(KEY) {
            let start = at + KEY.len();
            let end = command[start..]
                .char_indices()
                .find(|&(_, c)| c == ',' || c == '"' || (!quoted && c.is_whitespace()))
                .map_or(command.len(), |(offset, _)| start + offset);
            values.push((start, end));
            at = end;
            continue;
        }
        let c = command[at..].chars().next().unwrap_or(' ');
        if c == '"' {
            quoted = !quoted;
        }
        at += c.len_utf8();
    }
    values
}

/// `command` as it is saved: each `password=` value replaced by [`SAVED_PASSWORD`]; returned are
/// the values in order, `None` where the command already carried the marker (keep the stored one).
pub fn conceal_passwords(command: &str) -> (String, Vec<Option<String>>) {
    let mut saved = String::with_capacity(command.len());
    let mut values = Vec::new();
    let mut last = 0;
    for (start, end) in password_values(command) {
        let value = &command[start..end];
        values.push((value != SAVED_PASSWORD).then(|| value.to_string()));
        saved.push_str(&command[last..start]);
        saved.push_str(SAVED_PASSWORD);
        last = end;
    }
    saved.push_str(&command[last..]);
    (saved, values)
}

/// The saved `command` with its markers replaced, in order, by the stored `passwords` (a marker
/// without one stays as it is).
pub fn reveal_passwords(command: &str, passwords: &[String]) -> String {
    let mut revealed = String::with_capacity(command.len());
    let mut last = 0;
    for ((start, end), password) in password_values(command).into_iter().zip(passwords) {
        if &command[start..end] != SAVED_PASSWORD {
            continue;
        }
        revealed.push_str(&command[last..start]);
        revealed.push_str(password);
        last = end;
    }
    revealed.push_str(&command[last..]);
    revealed
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
    fn a_custom_command_password_is_saved_as_a_marker_and_revealed_for_the_run() {
        let command = "katago contribute -override-config \"username=a, password=x y,z\",\"password=p2\" password=p3 -x";
        let (saved, values) = conceal_passwords(command);
        assert_eq!(
            saved,
            "katago contribute -override-config \"username=a, password=******,z\",\"password=******\" password=****** -x"
        );
        assert_eq!(values, vec![Some("x y".into()), Some("p2".into()), Some("p3".into())]);
        let stored: Vec<String> = values.into_iter().flatten().collect();
        assert_eq!(reveal_passwords(&saved, &stored), command);
        // Saving the shown command again keeps the stored values.
        assert_eq!(conceal_passwords(&saved), (saved.clone(), vec![None, None, None]));
    }
}
