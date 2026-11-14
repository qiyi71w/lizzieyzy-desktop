//! The Ubuntu deb's update handover. A deb installation's files belong to the system package
//! manager, so this program never writes them: the verified package is installed by `apt-get`
//! after the system's own authorization prompt (polkit's `pkexec`). The program keeps running
//! meanwhile and restarts only once the package manager reports the new version installed.
//!
//! This module does no I/O: from what the platform adapter read with `dpkg-deb` and `dpkg-query`
//! it decides whether a package is this installation's update, and what a finished handover did.

/// A deb package: a package file's control fields, or the installed package's record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Package {
    pub name: String,
    pub version: String,
    pub architecture: String,
    /// `dpkg-query`'s `${Status}` of an installed package; `None` for a package file.
    pub status: Option<String>,
}

impl Package {
    /// Reads `Package`, `Version`, `Architecture` and (for an installed package) `Status` from
    /// `Field: value` lines, as `dpkg-deb --field` and `dpkg-query --status` print them.
    pub fn parse(text: &str) -> Option<Package> {
        let field = |name: &str| {
            text.lines()
                .find_map(|line| line.strip_prefix(name)?.strip_prefix(':'))
                .map(|value| value.trim().to_string())
                .filter(|value| !value.is_empty())
        };
        Some(Package {
            name: field("Package")?,
            version: field("Version")?,
            architecture: field("Architecture")?,
            status: field("Status"),
        })
    }

    /// Fully installed and configured: nothing is left for the package manager to finish.
    fn installed(&self) -> bool {
        self.status.as_deref() == Some("install ok installed")
    }

    /// Whether this package file is the offered `version` of the `installed` package, for the
    /// same architecture. Anything else is refused before authorization is asked for, so the
    /// administrator's consent is never spent on another package.
    pub fn updates(&self, installed: &Package, version: &str) -> bool {
        self.name == installed.name
            && self.version == version
            && (self.architecture == installed.architecture || self.architecture == "all")
    }
}

/// What a handover did, read from the package manager's records rather than from the command's
/// word alone: a download or a clean exit is never reported as an installation.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Handover {
    /// The offered version is installed: the program restarts into it.
    Installed,
    /// The authorization prompt was dismissed; nothing changed.
    Dismissed,
    /// Authorization was not granted or could not be asked for; nothing changed.
    Unauthorized,
    /// The package manager refused or failed; the installed package is as it was.
    NotInstalled,
    /// The package is not in an installed state: the installation was interrupted part-way.
    Unfinished,
}

/// `pkexec`'s exit code when the user dismissed the authorization prompt.
const DISMISSED: i32 = 126;
/// `pkexec`'s exit code when authorization was refused or no prompt could be shown.
const UNAUTHORIZED: i32 = 127;

/// Settles a handover of the `offered` version. `exit` is the exit code of `pkexec apt-get`
/// (`None` when it is unknown: ended by a signal, or the program was closed meanwhile) and `now`
/// the installed package's record afterwards (`None` when the system has none).
pub fn settle(exit: Option<i32>, now: Option<&Package>, offered: &str) -> Handover {
    match now {
        Some(package) if package.installed() && package.version == offered => Handover::Installed,
        Some(package) if package.installed() => match exit {
            Some(DISMISSED) => Handover::Dismissed,
            Some(UNAUTHORIZED) => Handover::Unauthorized,
            _ => Handover::NotInstalled,
        },
        _ => Handover::Unfinished,
    }
}

/// The package manager's own reasons in its output: apt's `E:` lines, else the last line.
pub fn reasons(output: &str) -> Option<String> {
    let lines: Vec<&str> = output.lines().map(str::trim).filter(|l| !l.is_empty()).collect();
    let errors: Vec<&str> = lines.iter().filter_map(|l| l.strip_prefix("E: ")).take(3).collect();
    let text = if errors.is_empty() {
        lines.last().copied()?.to_string()
    } else {
        errors.join("\n")
    };
    Some(text.chars().take(600).collect())
}
