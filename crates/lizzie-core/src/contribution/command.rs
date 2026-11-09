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
}
