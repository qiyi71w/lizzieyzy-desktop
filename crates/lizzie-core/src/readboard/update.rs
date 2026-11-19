//! Hosted helper updates (Java `ReadBoardUpdateRequest`, `ReadBoardUpdateInstaller` and the
//! lines `LizzieFrame.handleReadBoardHostedUpdateRequest` sends back).
//!
//! The helper downloads its own new release and asks the host to install it with
//! `readboardUpdateReady\t<vX.Y.Z>\t<absolute zip path>`, because it cannot replace its own
//! running files. The host checks the package ([`validate`]), asks the user, answers
//! [`CANCELLED_LINE`] or [`INSTALLING_LINE`], stops the helper, swaps the helper directory for
//! the package contents ([`install`], keeping a backup and rolling back on failure) and starts
//! the helper again. Any failure before the install is reported with [`failed_line`].

use std::fmt;
use std::fs;
use std::io;
use std::path::{Path, PathBuf};

const READY_COMMAND: &str = "readboardUpdateReady";
/// Sent when the user accepted the update, before the helper is stopped.
pub const INSTALLING_LINE: &str = "readboardUpdateInstalling";
/// Sent when the user declined the update.
pub const CANCELLED_LINE: &str = "readboardUpdateCancelled";
const FAILED_PREFIX: &str = "readboardUpdateFailed\t";

const LEGACY_PACKAGE_PREFIX: &str = "readboard-github-release-";
const WEBVIEW2_PACKAGE_PREFIX: &str = "readboard-webview2-";
/// Files every package must carry at its install root.
const REQUIRED_FILES: [&str; 5] = [
    "readboard.exe",
    "readboard.dll",
    "readboard.runtimeconfig.json",
    "readboard.deps.json",
    "language_cn.txt",
];

/// `readboardUpdateFailed\t<message>` (Java `sendHostedUpdateFailed` /
/// `sanitizeHostedUpdateMessage`): line breaks and tabs become spaces; an empty message becomes
/// `readboard update failed`, because the helper ignores the line without one.
pub fn failed_line(message: &str) -> String {
    if message.trim().is_empty() {
        return format!("{FAILED_PREFIX}readboard update failed");
    }
    let flat: String = message.chars().map(|c| if matches!(c, '\r' | '\n' | '\t') { ' ' } else { c }).collect();
    format!("{FAILED_PREFIX}{}", flat.trim_matches(|c: char| c <= ' '))
}

/// A helper's request to install a downloaded package.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateRequest {
    version_tag: String,
    zip_path: PathBuf,
}

/// Java `VERSION_TAG_PATTERN` `^v\d+\.\d+\.\d+$`.
fn is_version_tag(tag: &str) -> bool {
    let Some(numbers) = tag.strip_prefix('v') else {
        return false;
    };
    let parts: Vec<&str> = numbers.split('.').collect();
    parts.len() == 3 && parts.iter().all(|p| !p.is_empty() && p.bytes().all(|b| b.is_ascii_digit()))
}

/// Java `isAbsolutePath`: absolute on this system, or a Windows drive path (`X:\…`, `X:/…`) or
/// UNC path (`\\…`) by text, so a request is judged the same way on every platform.
fn is_absolute_path(text: &str) -> bool {
    let bytes = text.as_bytes();
    let drive = bytes.len() >= 4
        && bytes[0].is_ascii_alphabetic()
        && bytes[1] == b':'
        && matches!(bytes[2], b'\\' | b'/');
    let unc = text.len() > 2 && text.starts_with("\\\\");
    Path::new(text).is_absolute() || drive || unc
}

impl UpdateRequest {
    /// Java `ReadBoardUpdateRequest.tryParse`: line breaks removed, exactly three tab-separated
    /// fields `readboardUpdateReady`, a `vX.Y.Z` tag and an absolute zip path.
    pub fn try_parse(line: &str) -> Option<UpdateRequest> {
        let cleaned: String = line.chars().filter(|&c| c != '\r' && c != '\n').collect();
        let parts: Vec<&str> = cleaned.split('\t').collect();
        if parts.len() != 3 || parts[0] != READY_COMMAND {
            return None;
        }
        let (tag, path) = (parts[1], parts[2]);
        if !is_version_tag(tag) || path.is_empty() || !is_absolute_path(path) {
            return None;
        }
        Some(UpdateRequest { version_tag: tag.to_string(), zip_path: PathBuf::from(path) })
    }

    pub fn version_tag(&self) -> &str {
        &self.version_tag
    }

    pub fn zip_path(&self) -> &Path {
        &self.zip_path
    }
}

/// Why a package was refused or could not be installed; the text is what
/// `readboardUpdateFailed` carries and what the user is shown.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct UpdateError(String);

impl UpdateError {
    fn new(message: &str) -> UpdateError {
        UpdateError(message.to_string())
    }

    pub fn message(&self) -> &str {
        &self.0
    }
}

impl fmt::Display for UpdateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(&self.0)
    }
}

impl std::error::Error for UpdateError {}

impl From<io::Error> for UpdateError {
    fn from(e: io::Error) -> UpdateError {
        UpdateError(e.to_string())
    }
}

impl From<zip::result::ZipError> for UpdateError {
    fn from(e: zip::result::ZipError) -> UpdateError {
        UpdateError(e.to_string())
    }
}

/// One file or directory of the package and where it goes below the install directory.
struct Entry {
    index: usize,
    /// `/`-separated, below the stripped top-level directory.
    relative: String,
    directory: bool,
}

struct Layout {
    zip_path: PathBuf,
    entries: Vec<Entry>,
}

/// Java `normalizeEntryPath`: backslashes become `/`, trailing separators are dropped (an empty
/// name is skipped); absolute, drive and `.`/`..`/empty segments are refused.
fn normalize_entry_name(raw: &str) -> Result<Option<String>, UpdateError> {
    let replaced = raw.replace('\\', "/");
    let name = replaced.trim_end_matches('/');
    if name.is_empty() {
        return Ok(None);
    }
    if name.starts_with('/') {
        return Err(UpdateError::new("ReadBoard update package contains an absolute path."));
    }
    let mut chars = name.chars();
    if chars.next().is_some_and(char::is_alphabetic) && chars.next() == Some(':') {
        return Err(UpdateError::new("ReadBoard update package contains a drive-relative path."));
    }
    if name.split('/').any(|s| s.is_empty() || s == "." || s == "..") {
        return Err(UpdateError::new("ReadBoard update package contains an unsafe relative path."));
    }
    Ok(Some(name.to_string()))
}

/// Java `inspectPackage`: the package exists, is named `readboard-github-release-<tag>.zip` or
/// `readboard-webview2-<tag>.zip`, has only safe entry names, and carries the required files at
/// its root — or inside its single top-level directory, which is then stripped.
fn inspect(request: &UpdateRequest) -> Result<Layout, UpdateError> {
    let zip_path = request.zip_path.clone();
    if !zip_path.is_file() {
        return Err(UpdateError::new("ReadBoard update package does not exist."));
    }
    let file_name = zip_path.file_name().map(|n| n.to_string_lossy()).unwrap_or_default();
    let tag = &request.version_tag;
    if file_name != format!("{LEGACY_PACKAGE_PREFIX}{tag}.zip") && file_name != format!("{WEBVIEW2_PACKAGE_PREFIX}{tag}.zip") {
        return Err(UpdateError::new("ReadBoard update package filename does not match the requested version."));
    }

    let mut zip = zip::ZipArchive::new(fs::File::open(&zip_path)?)?;
    let mut named: Vec<(usize, String, bool)> = Vec::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index)?;
        if let Some(name) = normalize_entry_name(entry.name())? {
            named.push((index, name, entry.name().ends_with('/')));
        }
    }
    if named.is_empty() {
        return Err(UpdateError::new("ReadBoard update package is empty."));
    }

    let root = |name: &str| name.split('/').next().unwrap_or("").to_string();
    let first_root = root(&named[0].1);
    let single_root = named.iter().all(|(_, name, _)| root(name) == first_root);
    let entries: Vec<Entry> = named
        .into_iter()
        .filter_map(|(index, name, directory)| {
            let relative = if single_root {
                // The top-level directory itself has nothing to install.
                name.strip_prefix(&first_root)?.strip_prefix('/')?.to_string()
            } else {
                name
            };
            Some(Entry { index, relative, directory })
        })
        .collect();

    let has = |file: &str| entries.iter().any(|e| !e.directory && e.relative == file);
    if !REQUIRED_FILES.into_iter().all(has) {
        return Err(UpdateError::new("ReadBoard update package is missing required runtime files."));
    }
    Ok(Layout { zip_path, entries })
}

/// Java `validateRequest`: checks the package without touching the install directory.
pub fn validate(request: &UpdateRequest) -> Result<(), UpdateError> {
    inspect(request).map(|_| ())
}

fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Java `extractPackage`: a fresh staging directory with the package contents.
fn extract(layout: &Layout, staging: &Path) -> Result<(), UpdateError> {
    remove_tree(staging)?;
    fs::create_dir_all(staging)?;
    let mut zip = zip::ZipArchive::new(fs::File::open(&layout.zip_path)?)?;
    for entry in &layout.entries {
        let target = entry.relative.split('/').fold(staging.to_path_buf(), |path, segment| path.join(segment));
        let below = target
            .strip_prefix(staging)
            .is_ok_and(|rest| rest.components().all(|c| matches!(c, std::path::Component::Normal(_))));
        if !below {
            return Err(UpdateError::new("ReadBoard update package contains an unsafe path."));
        }
        if entry.directory {
            fs::create_dir_all(&target)?;
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let mut source = zip.by_index(entry.index)?;
        let mut file = fs::File::create(&target)?;
        io::copy(&mut source, &mut file)?;
    }
    Ok(())
}

/// The install transaction (Java `install` after extraction): the current directory becomes the
/// backup, the staging directory becomes the install directory. If that fails, the staging
/// directory is removed and the backup is moved back when the install directory is missing.
/// The first failure is returned; cleanup and rollback failures do not replace it.
fn swap_in(
    staging: &Path,
    target: &Path,
    backup: &Path,
    rename: &mut dyn FnMut(&Path, &Path) -> io::Result<()>,
) -> io::Result<()> {
    let mut moved_current = false;
    let mut result = Ok(());
    if target.exists() {
        result = rename(target, backup);
        moved_current = result.is_ok();
    }
    if result.is_ok() {
        result = rename(staging, target);
    }
    if result.is_err() {
        let _ = remove_tree(staging);
        if moved_current && !target.exists() && backup.exists() {
            let _ = rename(backup, target);
        }
    }
    result
}

/// Java `ReadBoardUpdateInstaller.install`: checks the package, extracts it next to
/// `install_dir` into `readboard.staging-<time>`, renames the current directory to
/// `readboard.backup-<time>` (kept afterwards) and the staging directory to `install_dir`.
/// `<time>` is the local time `yyyyMMdd-HHmmss-SSS`. The helper must be stopped first, since its
/// files cannot be moved while it runs on Windows.
pub fn install(request: &UpdateRequest, install_dir: &Path) -> Result<(), UpdateError> {
    let layout = inspect(request)?;
    let target = std::path::absolute(install_dir)?;
    let parent = target
        .parent()
        .ok_or_else(|| UpdateError::new("ReadBoard install directory has no parent directory."))?;
    let suffix = chrono::Local::now().format("%Y%m%d-%H%M%S-%3f").to_string();
    let staging = parent.join(format!("readboard.staging-{suffix}"));
    let backup = parent.join(format!("readboard.backup-{suffix}"));
    extract(&layout, &staging)?;
    swap_in(&staging, &target, &backup, &mut |from: &Path, to: &Path| fs::rename(from, to))?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    const FLAT: [(&str, &str); 5] = [
        ("readboard.exe", "new-exe"),
        ("readboard.dll", "new-dll"),
        ("readboard.runtimeconfig.json", "{}"),
        ("readboard.deps.json", "{}"),
        ("language_cn.txt", "cn"),
    ];

    fn zip_at(path: &Path, entries: &[(&str, &str)]) {
        let mut zip = zip::ZipWriter::new(fs::File::create(path).unwrap());
        let options = zip::write::SimpleFileOptions::default();
        for (name, content) in entries {
            if name.ends_with('/') {
                zip.add_directory(*name, options).unwrap();
            } else {
                zip.start_file(*name, options).unwrap();
                zip.write_all(content.as_bytes()).unwrap();
            }
        }
        zip.finish().unwrap();
    }

    /// A request for `<dir>/<file_name>` holding `entries`, as the helper would send it.
    fn request(dir: &Path, file_name: &str, tag: &str, entries: &[(&str, &str)]) -> UpdateRequest {
        let path = dir.join(file_name);
        zip_at(&path, entries);
        UpdateRequest::try_parse(&format!("readboardUpdateReady\t{tag}\t{}", path.display())).unwrap()
    }

    fn prefixed(prefix: &str, entries: &[(&str, &str)]) -> Vec<(String, String)> {
        entries.iter().map(|(n, c)| (format!("{prefix}{n}"), c.to_string())).collect()
    }

    fn refs(entries: &[(String, String)]) -> Vec<(&str, &str)> {
        entries.iter().map(|(n, c)| (n.as_str(), c.as_str())).collect()
    }

    #[test]
    fn request_accepts_version_tag_and_absolute_zip_path() {
        let parsed =
            UpdateRequest::try_parse("readboardUpdateReady\tv3.0.2\tC:\\updates\\readboard-github-release-v3.0.2.zip\r\n")
                .unwrap();
        assert_eq!(parsed.version_tag(), "v3.0.2");
        assert_eq!(parsed.zip_path(), Path::new("C:\\updates\\readboard-github-release-v3.0.2.zip"));
        assert!(UpdateRequest::try_parse("readboardUpdateReady\tv10.20.30\td:/u/readboard-webview2-v10.20.30.zip").is_some());
        assert!(UpdateRequest::try_parse("readboardUpdateReady\tv3.2.0\t\\\\server\\share\\x.zip").is_some());
        let temp = tempfile::tempdir().unwrap();
        let native = temp.path().join("x.zip");
        assert!(UpdateRequest::try_parse(&format!("readboardUpdateReady\tv3.2.0\t{}", native.display())).is_some());
    }

    #[test]
    fn request_rejects_malformed_payloads() {
        for line in [
            "readboardUpdateReady",
            "readboardUpdateReady\t3.0.2\tC:\\updates\\readboard-github-release-v3.0.2.zip",
            "readboardUpdateReady\tv3.0.2\treadboard-github-release-v3.0.2.zip",
            "readboardUpdateReady\tv3.0.2\tC:\\a.zip\textra-field",
            "readboardUpdateReady\tv3.0\tC:\\a.zip",
            "readboardUpdateReady\tv3.0.2-beta\tC:\\a.zip",
            "readboardUpdateReady\tv3..2\tC:\\a.zip",
            "readboardUpdateReady\tv3.0.2\t",
            // A bare drive root is too short for the textual drive rule; Windows itself calls it
            // absolute (see below).
            #[cfg(not(windows))]
            "readboardUpdateReady\tv3.0.2\tC:\\",
            "readboardUpdateReady\tv3.0.2\t\\\\",
            "readboardUpdateReadyX\tv3.0.2\tC:\\a.zip",
            "readboardUpdateReady v3.0.2 C:\\a.zip",
        ] {
            assert_eq!(UpdateRequest::try_parse(line), None, "{line:?}");
        }
        // As Java's File.isAbsolute, Windows accepts its own drive root; install then fails
        // because it is not a package file.
        #[cfg(windows)]
        assert_eq!(
            UpdateRequest::try_parse("readboardUpdateReady\tv3.0.2\tC:\\").map(|r| r.zip_path().to_path_buf()),
            Some(PathBuf::from("C:\\"))
        );
    }

    #[test]
    fn failed_line_flattens_and_defaults_the_message() {
        assert_eq!(failed_line("bad\r\npackage\tname "), "readboardUpdateFailed\tbad  package name");
        assert_eq!(failed_line("  "), "readboardUpdateFailed\treadboard update failed");
        assert_eq!(INSTALLING_LINE, "readboardUpdateInstalling");
        assert_eq!(CANCELLED_LINE, "readboardUpdateCancelled");
    }

    #[test]
    fn validate_accepts_flat_top_directory_and_both_package_names() {
        let temp = tempfile::tempdir().unwrap();
        let flat = request(temp.path(), "readboard-github-release-v3.0.2.zip", "v3.0.2", &FLAT);
        assert_eq!(validate(&flat), Ok(()));

        let bundle = prefixed("bundle/", &FLAT);
        let mut with_dir = vec![("bundle/", "")];
        with_dir.extend(refs(&bundle));
        let nested = request(temp.path(), "readboard-webview2-v3.1.0.zip", "v3.1.0", &with_dir);
        assert_eq!(validate(&nested), Ok(()));

        let backslashes = prefixed("bundle\\", &FLAT);
        let windows = request(temp.path(), "readboard-github-release-v3.1.1.zip", "v3.1.1", &refs(&backslashes));
        assert_eq!(validate(&windows), Ok(()));
    }

    #[test]
    fn validate_rejects_wrong_names_missing_files_and_unsafe_entries() {
        let temp = tempfile::tempdir().unwrap();
        let custom = request(temp.path(), "readboard-custom-v3.1.0.zip", "v3.1.0", &FLAT);
        assert!(validate(&custom).unwrap_err().message().contains("filename does not match"));
        let other_version = request(temp.path(), "readboard-webview2-v3.1.0.zip", "v3.1.1", &FLAT);
        assert!(validate(&other_version).is_err());

        let deep = prefixed("bundle/bin/", &FLAT);
        let nested = request(temp.path(), "readboard-github-release-v3.0.2.zip", "v3.0.2", &refs(&deep));
        assert!(validate(&nested).unwrap_err().message().contains("missing required runtime files"));

        let missing = request(temp.path(), "readboard-github-release-v3.0.3.zip", "v3.0.3", &FLAT[..4]);
        assert!(validate(&missing).unwrap_err().message().contains("missing required runtime files"));

        let empty = request(temp.path(), "readboard-github-release-v3.0.4.zip", "v3.0.4", &[]);
        assert!(validate(&empty).unwrap_err().message().contains("is empty"));

        for (index, (bad, message)) in [
            ("../evil.txt", "unsafe relative path"),
            ("a/./b.txt", "unsafe relative path"),
            ("a//b.txt", "unsafe relative path"),
            ("/evil.txt", "absolute path"),
            ("\\evil.txt", "absolute path"),
            ("C:evil.txt", "drive-relative path"),
        ]
        .into_iter()
        .enumerate()
        {
            let mut entries = vec![(bad, "x")];
            entries.extend(FLAT);
            let tag = format!("v9.9.{index}");
            let unsafe_package =
                request(temp.path(), &format!("readboard-github-release-{tag}.zip"), &tag, &entries);
            let error = validate(&unsafe_package).unwrap_err();
            assert!(error.message().contains(message), "{bad}: {error}");
        }

        let absent = UpdateRequest::try_parse(&format!(
            "readboardUpdateReady\tv3.0.2\t{}",
            temp.path().join("readboard-github-release-v3.0.5.zip").display()
        ))
        .unwrap();
        assert!(validate(&absent).unwrap_err().message().contains("does not exist"));
    }

    #[test]
    fn install_replaces_existing_directory_and_keeps_backup() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("readboard");
        fs::create_dir_all(&install_dir).unwrap();
        fs::write(install_dir.join("old.txt"), "old").unwrap();
        fs::write(install_dir.join("readboard.exe"), "old-exe").unwrap();
        let downloads = temp.path().join("downloads");
        fs::create_dir_all(&downloads).unwrap();
        let bundle = prefixed("readboard-webview2-v3.2.0/", &FLAT);
        let mut entries = refs(&bundle);
        entries.push(("readboard-webview2-v3.2.0/runtimes/win-x64/native/x.dll", "native"));
        let update = request(&downloads, "readboard-webview2-v3.2.0.zip", "v3.2.0", &entries);

        install(&update, &install_dir).unwrap();

        assert_eq!(fs::read_to_string(install_dir.join("readboard.exe")).unwrap(), "new-exe");
        assert_eq!(fs::read_to_string(install_dir.join("readboard.dll")).unwrap(), "new-dll");
        assert_eq!(fs::read_to_string(install_dir.join("runtimes/win-x64/native/x.dll")).unwrap(), "native");
        assert!(!install_dir.join("old.txt").exists());
        let names: Vec<String> =
            fs::read_dir(temp.path()).unwrap().map(|e| e.unwrap().file_name().to_string_lossy().into_owned()).collect();
        let backup = names.iter().find(|n| n.starts_with("readboard.backup-")).expect("backup directory");
        assert_eq!(fs::read_to_string(temp.path().join(backup).join("old.txt")).unwrap(), "old");
        assert!(!names.iter().any(|n| n.starts_with("readboard.staging-")));
    }

    #[test]
    fn install_without_previous_directory_creates_it() {
        let temp = tempfile::tempdir().unwrap();
        let install_dir = temp.path().join("app").join("readboard");
        fs::create_dir_all(temp.path().join("app")).unwrap();
        let update = request(temp.path(), "readboard-github-release-v3.0.2.zip", "v3.0.2", &FLAT);
        install(&update, &install_dir).unwrap();
        assert_eq!(fs::read_to_string(install_dir.join("language_cn.txt")).unwrap(), "cn");
        let backups = fs::read_dir(temp.path().join("app"))
            .unwrap()
            .filter(|e| e.as_ref().unwrap().file_name().to_string_lossy().starts_with("readboard.backup-"))
            .count();
        assert_eq!(backups, 0);
    }

    #[test]
    fn failed_swap_restores_the_previous_directory() {
        let temp = tempfile::tempdir().unwrap();
        let target = temp.path().join("readboard");
        let staging = temp.path().join("readboard.staging-1");
        let backup = temp.path().join("readboard.backup-1");
        fs::create_dir_all(&target).unwrap();
        fs::write(target.join("readboard.exe"), "old-exe").unwrap();
        fs::create_dir_all(&staging).unwrap();
        fs::write(staging.join("readboard.exe"), "new-exe").unwrap();

        let mut calls = 0;
        let error = swap_in(&staging, &target, &backup, &mut |from: &Path, to: &Path| {
            calls += 1;
            if calls == 2 {
                return Err(io::Error::new(io::ErrorKind::PermissionDenied, "locked"));
            }
            fs::rename(from, to)
        })
        .unwrap_err();

        assert_eq!(error.to_string(), "locked");
        assert_eq!(calls, 3);
        assert_eq!(fs::read_to_string(target.join("readboard.exe")).unwrap(), "old-exe");
        assert!(!staging.exists());
        assert!(!backup.exists());
    }
}
