//! Line handling of the contribution process (`ContributeEngine.classify` and `parseTips`):
//! permanent failures, the recorded error, the tip line, and normal-exit markers.

use crate::sanitize::sanitize;

/// `ContributeEngine.Failure`.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum Failure {
    #[default]
    Other,
    /// The engine was built without distributed-training support.
    DistributedUnsupported,
    /// The server refused this build.
    BuildRejected,
    /// KataGo's bundled CA certificates were not found.
    CaCertificates,
}

impl Failure {
    /// These end the session at once, without a retry.
    pub fn permanent(self) -> bool {
        matches!(
            self,
            Failure::DistributedUnsupported | Failure::BuildRejected
        )
    }
}

/// `EngineStartupDiagnostic.bounded`: at most `limit` characters.
pub fn bounded(text: &str, limit: usize) -> String {
    text.chars().take(limit).collect()
}

/// What `classify` found in a line.
#[derive(Clone, Debug, PartialEq)]
pub struct Classified {
    pub failure: Failure,
    /// The error text the session records, if this line is one.
    pub error: Option<String>,
}

/// `classify`: permanent build failures first, the CA hint, then any line that looks like an
/// error. `text` resolves the Java message keys.
pub fn classify(line: &str, text: &dyn Fn(&str) -> String) -> Classified {
    let (failure, key) = if line.contains("Compile with -DBUILD_DISTRIBUTED") {
        (
            Failure::DistributedUnsupported,
            "Contribute.tips.useOfficialEngineSupportDistributedTraining",
        )
    } else if line.contains("This version of KataGo is not enabled for distributed") {
        (Failure::BuildRejected, "Contribute.tips.buildRejected")
    } else if line.contains("not find CA certs") {
        (Failure::CaCertificates, "Contribute.tips.noCAcerts")
    } else {
        let looks_like_error =
            line.to_lowercase().contains("error") || line.starts_with("This ");
        return Classified {
            failure: Failure::Other,
            error: looks_like_error.then(|| bounded(&sanitize(line), 4096)),
        };
    };
    Classified {
        failure,
        error: Some(text(key)),
    }
}

/// One tip update; `Eternal` is overwritten only by pause/resume tips, `PauseResume` always.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum TipKind {
    Normal,
    Eternal,
    PauseResume,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Tip {
    pub key: &'static str,
    pub kind: TipKind,
}

/// `parseTips` for one line. Everything the line matches applies in Java's order (the last tip
/// wins), the known error texts are the recorded failure message, and `normal_end` marks
/// `Exited cleanly after signal` / `All cleaned up, quitting`.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct Tips {
    pub tips: Vec<Tip>,
    pub errors: Vec<&'static str>,
    pub normal_end: bool,
    pub starting_game: bool,
}

const ERRORS: [(&str, &str); 8] = [
    ("Invalid username/password", "Contribute.tips.invalidUserNamePassword"),
    ("No response from server", "Contribute.tips.noResponseFromServer"),
    ("Error connecting to server", "Contribute.tips.errorConnectingToServer"),
    (" When uploading", "Contribute.tips.uploadError"),
    ("status 200 for initial query", "Contribute.tips.initialQuery"),
    ("Could not parse serverUrl", "Contribute.tips.couldNotParseServerUrl"),
    (
        "Did you verify your email address",
        "Contribute.tips.didYouVerifyYourEmailAddress",
    ),
    (
        "Model file was incompletely downloaded",
        "Contribute.tips.modelFileWasIncompletelyDownloaded",
    ),
];

/// `parseTips`.
pub fn tips(line: &str) -> Tips {
    let mut out = Tips {
        starting_game: starting_game(line),
        ..Tips::default()
    };
    if out.starting_game {
        out.tips.push(Tip {
            key: "Contribute.tips.startingNewGame",
            kind: TipKind::Normal,
        });
    }
    let lower = line.to_lowercase();
    if lower.contains("predownload") {
        out.tips.push(Tip {
            key: "Contribute.tips.predownload",
            kind: TipKind::Normal,
        });
    } else if lower.contains("download") {
        out.tips.push(Tip {
            key: "Contribute.tips.download",
            kind: TipKind::Normal,
        });
    }
    for (needle, key) in ERRORS {
        if line.contains(needle) {
            out.tips.push(Tip {
                key,
                kind: TipKind::Normal,
            });
            out.errors.push(key);
        }
    }
    if line.contains("Signal to stop") {
        out.tips.push(Tip {
            key: "Contribute.tips.signalToStop",
            kind: TipKind::Eternal,
        });
    }
    if line.contains("Exited cleanly after signal") || line.contains("All cleaned up, quitting") {
        out.normal_end = true;
    }
    if line.contains("Pausing contribute") {
        out.tips.push(Tip {
            key: "Contribute.tips.PauseHaveSent",
            kind: TipKind::PauseResume,
        });
    }
    if line.contains("Resuming contribute") {
        out.tips.push(Tip {
            key: "Contribute.tips.ResumeHaveSent",
            kind: TipKind::PauseResume,
        });
    }
    out
}

/// Java `STARTING_GAME`: an optional `yyyy-MM-dd HH:mm:ss…: ` prefix, `Starting game`, digits.
pub fn starting_game(line: &str) -> bool {
    let rest = timestamp_prefix(line).unwrap_or(line);
    let Some(at) = rest.find("Starting game ") else {
        return false;
    };
    let after = &rest[at + "Starting game ".len()..];
    after.starts_with(|c: char| c.is_ascii_digit())
}

/// The line after an optional Java log prefix `yyyy-MM-dd HH:mm:ss[^ ]*: `.
fn timestamp_prefix(line: &str) -> Option<&str> {
    let bytes = line.as_bytes();
    let digit = |i: usize| bytes.get(i).is_some_and(u8::is_ascii_digit);
    for i in [0, 1, 2, 3, 5, 6, 8, 9, 11, 12, 14, 15, 17, 18] {
        if !digit(i) {
            return None;
        }
    }
    for (i, expected) in [(4, b'-'), (7, b'-'), (10, b' '), (13, b':'), (16, b':')] {
        if bytes.get(i) != Some(&expected) {
            return None;
        }
    }
    // `[^ ]*: ` after the seconds; the rest of the line follows that delimiter.
    let mut i = 19;
    while i + 1 < bytes.len() {
        if bytes[i] == b':' && bytes[i + 1] == b' ' {
            return Some(&line[i + 2..]);
        }
        if bytes[i] == b' ' {
            return None;
        }
        i += 1;
    }
    None
}

#[cfg(test)]
mod tests {
    use super::*;

    fn text(key: &str) -> String {
        key.to_string()
    }

    #[test]
    fn permanent_failures_carry_their_text_and_fail_the_session() {
        let c = classify("This version of KataGo was NOT compiled with support. Compile with -DBUILD_DISTRIBUTED=1", &text);
        assert_eq!(c.failure, Failure::DistributedUnsupported);
        assert!(c.failure.permanent());
        let c = classify("Server returned error 400: This version of KataGo is not enabled for distributed.", &text);
        assert_eq!(c.failure, Failure::BuildRejected);
        let c = classify("Could not find CA certs", &text);
        assert_eq!(c.failure, Failure::CaCertificates);
        assert!(!c.failure.permanent());
    }

    #[test]
    fn other_errors_are_recorded_masked_and_bounded() {
        let c = classify("Unexpected error: password=hunter2", &text);
        assert_eq!(c.failure, Failure::Other);
        let error = c.error.unwrap();
        assert!(!error.contains("hunter2"));
        assert_eq!(bounded(&"x".repeat(5000), 4096).chars().count(), 4096);
    }

    #[test]
    fn starting_game_accepts_the_java_log_prefix() {
        assert!(starting_game("Starting game 3 (training)"));
        assert!(starting_game("2022-03-14 09:00:41+0800: Starting game 12"));
        assert!(!starting_game("2022-03-14 Starting game"));
        assert!(!starting_game("Exited cleanly after signal. Starting game later"));
    }

    #[test]
    fn tips_keep_java_order_and_the_known_errors() {
        let t = tips("2022-03-14 09:00:41+0800: Pausing contribute");
        assert_eq!(t.tips.last().unwrap().key, "Contribute.tips.PauseHaveSent");
        assert_eq!(t.tips.last().unwrap().kind, TipKind::PauseResume);
        let t = tips("Signal to stop");
        assert_eq!(t.tips.last().unwrap().kind, TipKind::Eternal);
        let t = tips("Invalid username/password. Please check.");
        assert_eq!(t.errors, vec!["Contribute.tips.invalidUserNamePassword"]);
        assert!(tips("All cleaned up, quitting").normal_end);
    }
}
