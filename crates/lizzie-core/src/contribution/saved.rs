//! `SavedEnginePathResolver` (`util/SavedEnginePathResolver.java`): what each configured engine
//! entry would fill into 引擎路径, and what this client knows about the qualification of an engine
//! path (`ContributionQualification`).

use std::path::{Path, PathBuf};

use crate::api::{
    ContributionQualification, SavedEngineCandidate, SavedEngineKind, SavedEngineReason,
};
use crate::engine::gtp::{prepare_launch, split_command};
use crate::engine::profile::EngineProfile;
use crate::resources::{Catalog, ResourceStore};

/// `CommandLaunchHelper.isIndirectLauncher`.
pub fn is_indirect_launcher(command: &str) -> bool {
    let base = command.rsplit(['/', '\\']).next().unwrap_or("");
    let mut executable = base.to_lowercase();
    if let Some(stripped) = executable.strip_suffix(".exe") {
        executable = stripped.to_string();
    } else if [".bat", ".cmd", ".ps1", ".sh"]
        .iter()
        .any(|ending| executable.ends_with(ending))
    {
        return true;
    }
    if interpreter_host(&executable) {
        return true;
    }
    matches!(
        executable.as_str(),
        "ssh"
            | "plink"
            | "wsl"
            | "wslhost"
            | "docker"
            | "podman"
            | "wine"
            | "wine64"
            | "flatpak"
            | "snap"
            | "cmd"
            | "powershell"
            | "pwsh"
            | "sh"
            | "bash"
            | "zsh"
            | "fish"
            | "env"
            | "nohup"
    )
}

fn interpreter_host(executable: &str) -> bool {
    if matches!(
        executable,
        "py" | "nodejs" | "bun" | "deno" | "cscript" | "wscript" | "dotnet" | "mono"
    ) {
        return true;
    }
    [
        "pythonw", "python", "pypy", "javaw", "java", "node", "ruby", "perl", "php",
    ]
    .iter()
    .any(|base| has_numeric_version_suffix(executable, base))
}

/// A base name followed by digits and dots (`python3.11`, `node20`); the bare base counts too.
fn has_numeric_version_suffix(executable: &str, base: &str) -> bool {
    let Some(rest) = executable.strip_prefix(base) else {
        return false;
    };
    if rest.is_empty() {
        return true;
    }
    let mut saw_digit = false;
    let mut previous_dot = false;
    for (index, value) in rest.char_indices() {
        if value.is_ascii_digit() {
            saw_digit = true;
            previous_dot = false;
        } else if value == '.'
            && saw_digit
            && !previous_dot
            && index + 1 < rest.len()
        {
            previous_dot = true;
        } else {
            return false;
        }
    }
    saw_digit
}

/// `Utils.resolveExistingExecutable`: a readable file as written (relative to the process
/// directory), else a `PATH` lookup (with `PATHEXT` on Windows).
pub fn resolve_executable(executable: &str) -> Option<PathBuf> {
    let trimmed = executable.trim();
    if trimmed.is_empty() {
        return None;
    }
    let direct = Path::new(trimmed);
    let absolute = if direct.is_absolute() {
        direct.to_path_buf()
    } else {
        std::env::current_dir().ok()?.join(direct)
    };
    if absolute.is_file() {
        return Some(absolute);
    }
    let path = std::env::var_os("PATH")?;
    let mut suffixes: Vec<String> = Vec::new();
    if cfg!(windows) {
        if let Some(pathext) = std::env::var_os("PATHEXT") {
            suffixes.extend(
                pathext
                    .to_string_lossy()
                    .split(';')
                    .map(str::trim)
                    .filter(|s| !s.is_empty())
                    .map(|s| s.to_lowercase()),
            );
        }
    }
    for directory in std::env::split_paths(&path) {
        let candidate = directory.join(trimmed);
        if candidate.is_file() {
            return Some(candidate);
        }
        for suffix in &suffixes {
            let with = directory.join(format!("{trimmed}{suffix}"));
            if with.is_file() {
                return Some(with);
            }
        }
    }
    None
}

/// `Candidate.fieldText`: quoted so the contribution command splits it back into one argument.
pub fn field_text(executable: &Path) -> String {
    let path = executable.to_string_lossy();
    if path
        .chars()
        .any(|c| matches!(c, ' ' | '\t' | '\'' | '"'))
    {
        format!("\"{}\"", path.replace('"', "\"'\"'\""))
    } else {
        path.into_owned()
    }
}

fn unavailable(
    profile: &EngineProfile,
    index: u32,
    kind: SavedEngineKind,
    reason: SavedEngineReason,
) -> SavedEngineCandidate {
    SavedEngineCandidate {
        index,
        name: profile.name.trim().to_string(),
        kind,
        executable: None,
        field_text: None,
        reason: Some(reason),
    }
}

/// `SavedEnginePathResolver.resolveAll`: one candidate per entry, in list order, from 1.
pub fn candidates(engines: &[EngineProfile], roots: &[PathBuf]) -> Vec<SavedEngineCandidate> {
    engines
        .iter()
        .enumerate()
        .map(|(position, profile)| resolve(profile, position as u32 + 1, roots))
        .collect()
}

fn resolve(profile: &EngineProfile, index: u32, roots: &[PathBuf]) -> SavedEngineCandidate {
    if profile.is_remote() {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Remote,
            SavedEngineReason::RemoteTransport,
        );
    }
    let command = profile.command.trim();
    if command.is_empty() {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Unknown,
            SavedEngineReason::EmptyCommand,
        );
    }
    let tokens = split_command(command);
    if tokens.first().is_none_or(|token| token.trim().is_empty()) {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Unknown,
            SavedEngineReason::UnparseableCommand,
        );
    }
    if is_indirect_launcher(tokens[0].trim()) {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Indirect,
            SavedEngineReason::IndirectLauncher,
        );
    }
    let program = prepare_launch(command, roots)
        .map(|spec| spec.program)
        .unwrap_or_else(|| tokens[0].clone());
    let Some(executable) = resolve_executable(&program) else {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Local,
            SavedEngineReason::MissingFile,
        );
    };
    let executable = std::path::absolute(&executable)
        .unwrap_or(executable)
        .normalize_lexically();
    if is_indirect_launcher(&executable.to_string_lossy()) {
        return unavailable(
            profile,
            index,
            SavedEngineKind::Indirect,
            SavedEngineReason::IndirectLauncher,
        );
    }
    SavedEngineCandidate {
        index,
        name: profile.name.trim().to_string(),
        kind: SavedEngineKind::Local,
        field_text: Some(field_text(&executable)),
        executable: Some(executable.to_string_lossy().into_owned()),
        reason: None,
    }
}

trait NormalizeLexically {
    fn normalize_lexically(self) -> PathBuf;
}

impl NormalizeLexically for PathBuf {
    fn normalize_lexically(self) -> PathBuf {
        let mut out = PathBuf::new();
        for component in self.components() {
            match component {
                std::path::Component::ParentDir => {
                    out.pop();
                }
                std::path::Component::CurDir => {}
                other => out.push(other.as_os_str()),
            }
        }
        out
    }
}

/// What this client knows about `engine_path`'s 跑谱贡献 capability: only an engine package this
/// client installed is known; everything else is `Unknown` (never inferred from 官方 origin or
/// analysis support). The field is read as the command line reads it (a chooser or 已配置引擎
/// path is quoted), so the program that would run is the one qualified.
pub fn qualification(engine_path: &str, resources_dir: &Path) -> ContributionQualification {
    let Some(program) = crate::engine::gtp::split_command(engine_path.trim()).into_iter().next() else {
        return ContributionQualification::Unknown;
    };
    let trimmed = program.as_str();
    if trimmed.is_empty() {
        return ContributionQualification::Unknown;
    }
    let wanted = std::fs::canonicalize(trimmed).ok();
    let store = ResourceStore::new(resources_dir);
    let Ok(inventory) = store.inventory() else {
        return ContributionQualification::Unknown;
    };
    let Ok(catalog) = Catalog::embedded() else {
        return ContributionQualification::Unknown;
    };
    for package in inventory.packages {
        let same = match (&wanted, std::fs::canonicalize(&package.executable).ok()) {
            (Some(a), Some(b)) => *a == b,
            _ => package.executable.as_path() == Path::new(trimmed),
        };
        if !same {
            continue;
        }
        let Some(asset) = catalog.assets.get(&package.receipt.asset_id) else {
            continue;
        };
        return ContributionQualification::Managed {
            asset_id: asset.id.clone(),
            version: package.receipt.version.clone(),
            backend: asset.backend.clone(),
            status: asset.contribution.clone(),
        };
    }
    ContributionQualification::Unknown
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn indirect_launchers_follow_the_java_list() {
        for command in ["cmd", "python3", "python3.11", "java", "C:/x/run.bat", "bash", "ssh"] {
            assert!(is_indirect_launcher(command), "{command}");
        }
        for command in ["/opt/katago", "C:/KataGo/katago.exe", "katago"] {
            assert!(!is_indirect_launcher(command), "{command}");
        }
    }

    #[test]
    fn field_text_quotes_only_when_needed() {
        assert_eq!(field_text(Path::new("/opt/katago")), "/opt/katago");
        assert_eq!(field_text(Path::new("/opt/a b/katago")), "\"/opt/a b/katago\"");
        assert_eq!(field_text(Path::new("/o\"x/katago")), "\"/o\"'\"'\"x/katago\"");
    }

    #[test]
    fn a_remote_entry_is_reported_as_such() {
        let profile = EngineProfile {
            name: " remote ".into(),
            command: "katago gtp".into(),
            ssh: Some(Default::default()),
            ..EngineProfile::default()
        };
        let found = candidates(&[profile], &[]);
        assert_eq!(found[0].kind, SavedEngineKind::Remote);
        assert_eq!(found[0].name, "remote");
        assert_eq!(found[0].reason, Some(SavedEngineReason::RemoteTransport));
    }

    #[test]
    fn an_uninstalled_path_is_unknown() {
        let dir = tempfile::tempdir().unwrap();
        assert_eq!(
            qualification("/definitely/not/installed/katago", dir.path()),
            ContributionQualification::Unknown
        );
    }
}
