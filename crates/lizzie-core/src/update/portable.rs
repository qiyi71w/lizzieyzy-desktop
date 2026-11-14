//! Updating the Windows portable zip (Java `WindowsUpdateService.downloadAndPrepare` and the
//! separate `WindowsUpdateApplier`): the program folder's files are replaced by those of the new
//! zip while the program is closed, and the folder stays a portable folder.
//!
//! The work happens in three steps, all inside the program folder's work folder
//! [`WORK_DIR_NAME`], on the same volume as the files it replaces:
//!
//! 1. [`prepare`], while the program still runs: checks the verified package's layout, unpacks
//!    it into `new/` and records the files to replace. A folder that cannot be written fails
//!    here, before the program closes.
//! 2. [`apply`], from a helper process once the program has closed: for each file, the current
//!    one is kept in `backup/` (a hard link, else a copy) and the new one is renamed over it; the
//!    program itself is replaced last. Any failure puts every replaced file back.
//! 3. [`recover`], at the next start: finishes the work an interrupted helper left, or puts the
//!    old files back.
//!
//! Only files the package contains are written: settings and records (in the user profile) and
//! anything else in the folder, such as ReadBoard's own settings, stay as they are.

use serde::{Deserialize, Serialize};
use std::fs::{self, File};
use std::io;
use std::path::{Path, PathBuf};
use std::thread;
use std::time::Duration;

/// The work folder inside the program folder.
pub const WORK_DIR_NAME: &str = ".lizzieyzy-update";
const JOURNAL: &str = "journal.json";
const LOCK: &str = "lock";
const NEW: &str = "new";
const BACKUP: &str = "backup";
/// How long a file in use is retried before the update gives up (a virus scanner briefly
/// holding a new file, for example).
const IN_USE_RETRIES: u32 = 20;
const IN_USE_WAIT: Duration = Duration::from_millis(250);

/// Why the portable update did not happen. The program folder is unchanged unless the failure
/// is [`PortableFailure::Unrecovered`].
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum PortableFailure {
    /// The package is not a program zip of this program.
    Package,
    /// The program folder cannot be written.
    Permission,
    /// A program file is in use and could not be replaced.
    InUse,
    /// Another file system failure.
    Io,
    /// Putting the old files back failed too; the work folder stays for the next start.
    Unrecovered,
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct PortableError {
    pub failure: PortableFailure,
    pub detail: String,
}

impl PortableError {
    fn new(failure: PortableFailure, detail: impl Into<String>) -> Self {
        PortableError {
            failure,
            detail: detail.into(),
        }
    }
}

impl std::fmt::Display for PortableError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:?}: {}", self.failure, self.detail)
    }
}

/// What [`recover`] found at start.
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Recovery {
    /// No unfinished update.
    None,
    /// An interrupted update had already replaced every file; it is now finished.
    Completed,
    /// An interrupted update was undone: the program folder is as before.
    RolledBack,
    /// An interrupted update could not be undone (a file is in use); it is tried again at the
    /// next start.
    Failed(String),
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
enum Stage {
    /// Unpacked; nothing in the program folder has changed.
    Prepared,
    /// Files are being replaced.
    Applying,
    /// Every file is replaced.
    Committed,
}

#[derive(Debug, Serialize, Deserialize)]
struct Journal {
    version: String,
    /// The program's file name in the folder.
    program: String,
    /// Files to replace, `/`-separated and relative to the folder; the program is last.
    files: Vec<String>,
    stage: Stage,
    /// Folders the update creates, recorded when it starts so undoing it can remove them.
    #[serde(default)]
    created_dirs: Vec<String>,
}

fn work_dir(folder: &Path) -> PathBuf {
    folder.join(WORK_DIR_NAME)
}

fn below(base: &Path, relative: &str) -> PathBuf {
    relative.split('/').fold(base.to_path_buf(), |p, s| p.join(s))
}

fn read_journal(work: &Path) -> io::Result<Journal> {
    let text = fs::read_to_string(work.join(JOURNAL))?;
    serde_json::from_str(&text).map_err(|e| io::Error::new(io::ErrorKind::InvalidData, e))
}

/// Written beside and renamed over the journal, so a crash leaves the old or the new one.
fn write_journal(work: &Path, journal: &Journal) -> io::Result<()> {
    let text = serde_json::to_vec_pretty(journal).map_err(io::Error::other)?;
    let temp = work.join(format!("{JOURNAL}.tmp"));
    {
        let mut file = File::create(&temp)?;
        io::Write::write_all(&mut file, &text)?;
        file.sync_all()?;
    }
    fs::rename(temp, work.join(JOURNAL))
}

fn remove_tree(path: &Path) -> io::Result<()> {
    match fs::remove_dir_all(path) {
        Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
        other => other,
    }
}

/// Windows reports a file another process keeps open as a sharing or lock violation, and a
/// running program it cannot replace as access denied.
fn in_use(e: &io::Error) -> bool {
    matches!(e.raw_os_error(), Some(32 | 33)) || (cfg!(windows) && e.kind() == io::ErrorKind::PermissionDenied)
}

fn retrying(mut op: impl FnMut() -> io::Result<()>) -> io::Result<()> {
    let mut tries = 0;
    loop {
        match op() {
            Err(e) if in_use(&e) && tries < IN_USE_RETRIES => {
                tries += 1;
                thread::sleep(IN_USE_WAIT);
            }
            other => return other,
        }
    }
}

/// The package's entry name as a safe relative path, `None` for a folder entry.
fn entry_name(raw: &str) -> Result<Option<String>, PortableError> {
    let replaced = raw.replace('\\', "/");
    if replaced.ends_with('/') {
        return Ok(None);
    }
    let unsafe_name = replaced.starts_with('/')
        || replaced.chars().nth(1) == Some(':')
        || replaced.split('/').any(|s| s.is_empty() || s == "." || s == "..");
    if unsafe_name {
        return Err(PortableError::new(PortableFailure::Package, format!("unsafe entry {raw}")));
    }
    Ok(Some(replaced))
}

/// Takes the work folder's lock; `None` when a helper holds it.
fn lock(work: &Path) -> io::Result<Option<File>> {
    let file = File::options().create(true).truncate(false).write(true).open(work.join(LOCK))?;
    match file.try_lock() {
        Ok(()) => Ok(Some(file)),
        Err(fs::TryLockError::WouldBlock) => Ok(None),
        Err(fs::TryLockError::Error(e)) => Err(e),
    }
}

/// Unpacks the verified `package` for the program folder `folder`, whose program file is
/// `program` (it may have been renamed). The package holds one folder with the program
/// `binary` and its other files, as the release's portable zip does. Nothing outside the work
/// folder changes; an earlier unfinished preparation is replaced.
pub fn prepare(package: &Path, folder: &Path, program: &str, binary: &str, version: &str) -> Result<(), PortableError> {
    let package_error = |e: &dyn std::fmt::Display| PortableError::new(PortableFailure::Package, e.to_string());
    let mut zip = zip::ZipArchive::new(File::open(package).map_err(|e| package_error(&e))?).map_err(|e| package_error(&e))?;
    let mut names = Vec::new();
    for index in 0..zip.len() {
        let entry = zip.by_index(index).map_err(|e| package_error(&e))?;
        if entry.is_symlink() {
            return Err(package_error(&format!("link entry {}", entry.name())));
        }
        if let Some(name) = entry_name(entry.name())? {
            names.push((index, name));
        }
    }
    // The release zip holds a single folder; its contents are the program folder's files.
    let root = names.first().and_then(|(_, n)| n.split_once('/')).map(|(r, _)| r.to_string());
    let Some(root) = root.filter(|r| names.iter().all(|(_, n)| n.starts_with(&format!("{r}/")))) else {
        return Err(package_error(&"the package is not one program folder"));
    };
    let mut files: Vec<(usize, String)> = Vec::new();
    let mut program_entry = None;
    for (index, name) in names {
        let relative = name[root.len() + 1..].to_string();
        if relative == binary {
            program_entry = Some(index);
            continue;
        }
        let first = relative.split('/').next().unwrap_or("");
        if first.eq_ignore_ascii_case(WORK_DIR_NAME) || relative.eq_ignore_ascii_case(program) {
            return Err(package_error(&format!("reserved entry {relative}")));
        }
        if files.iter().any(|(_, f)| f.eq_ignore_ascii_case(&relative)) {
            return Err(package_error(&format!("duplicate entry {relative}")));
        }
        files.push((index, relative));
    }
    let Some(program_entry) = program_entry else {
        return Err(package_error(&format!("the package has no {binary}")));
    };
    files.push((program_entry, program.to_string()));

    let work = work_dir(folder);
    if let Ok(journal) = read_journal(&work) {
        if journal.stage == Stage::Applying {
            return Err(PortableError::new(PortableFailure::Io, "an earlier update is unfinished"));
        }
    }
    remove_tree(&work).map_err(|e| PortableError::new(classify_prepare(&e), e.to_string()))?;
    let result = (|| -> io::Result<()> {
        fs::create_dir_all(work.join(NEW))?;
        for (index, relative) in &files {
            let target = below(&work.join(NEW), relative);
            if let Some(parent) = target.parent() {
                fs::create_dir_all(parent)?;
            }
            let mut entry = zip.by_index(*index).map_err(io::Error::other)?;
            let mut file = File::create(&target)?;
            io::copy(&mut entry, &mut file)?;
            file.sync_all()?;
            #[cfg(unix)]
            {
                use std::os::unix::fs::PermissionsExt;
                let mode = if relative == program { 0o755 } else { entry.unix_mode().unwrap_or(0o644) & 0o777 };
                fs::set_permissions(&target, fs::Permissions::from_mode(mode))?;
            }
        }
        let journal = Journal {
            version: version.into(),
            program: program.into(),
            files: files.into_iter().map(|(_, f)| f).collect(),
            stage: Stage::Prepared,
            created_dirs: Vec::new(),
        };
        File::create(work.join(LOCK))?;
        write_journal(&work, &journal)
    })();
    result.map_err(|e| {
        let _ = remove_tree(&work);
        PortableError::new(classify_prepare(&e), e.to_string())
    })
}

/// While the program runs, an unwritable folder is a permission problem.
fn classify_prepare(e: &io::Error) -> PortableFailure {
    if matches!(e.raw_os_error(), Some(32 | 33)) {
        PortableFailure::InUse
    } else if e.kind() == io::ErrorKind::PermissionDenied {
        PortableFailure::Permission
    } else {
        PortableFailure::Io
    }
}

/// Removes a prepared update that will not be applied (the helper did not start).
pub fn discard(folder: &Path) {
    let work = work_dir(folder);
    if read_journal(&work).is_ok_and(|j| j.stage == Stage::Prepared) {
        let _ = remove_tree(&work);
    }
}

/// A prepared update claimed by the helper that applies it: while it is held, [`recover`] in a
/// program started meanwhile leaves the update alone.
pub struct Claim {
    folder: PathBuf,
    lock: File,
    journal: Journal,
}

/// Claims the prepared update of `folder`; the helper does this before it waits for the
/// program to close.
pub fn claim(folder: &Path) -> Result<Claim, PortableError> {
    let work = work_dir(folder);
    let io_error = |e: io::Error| PortableError::new(PortableFailure::Io, e.to_string());
    let Some(lock) = lock(&work).map_err(io_error)? else {
        return Err(PortableError::new(PortableFailure::Io, "another update is running"));
    };
    let journal = read_journal(&work).map_err(io_error)?;
    if journal.stage != Stage::Prepared {
        return Err(PortableError::new(PortableFailure::Io, format!("the update is {:?}", journal.stage)));
    }
    Ok(Claim {
        folder: folder.to_path_buf(),
        lock,
        journal,
    })
}

impl Claim {
    /// The version being installed.
    pub fn version(&self) -> &str {
        &self.journal.version
    }

    /// Replaces the program folder's files with the prepared ones; run once the program has
    /// closed. `replaced` hears each file as it is replaced. On failure every replaced file is
    /// put back and the work folder removed; if that fails too ([`PortableFailure::Unrecovered`])
    /// the work folder stays for [`recover`].
    pub fn apply(self, replaced: &mut dyn FnMut(&str)) -> Result<(), PortableError> {
        let Claim {
            folder,
            lock,
            mut journal,
        } = self;
        let folder = folder.as_path();
        let work = work_dir(folder);
        let io_error = |e: io::Error| PortableError::new(PortableFailure::Io, e.to_string());
        let mut created = Vec::new();
        for file in &journal.files {
            let mut dir = Path::new(file).parent();
            while let Some(d) = dir.filter(|d| !d.as_os_str().is_empty()) {
                let relative = d.to_string_lossy().replace('\\', "/");
                if !below(folder, &relative).exists() && !created.contains(&relative) {
                    created.push(relative);
                }
                dir = d.parent();
            }
        }
        created.sort_by_key(|d| d.len());
        journal.created_dirs = created;
        journal.stage = Stage::Applying;
        write_journal(&work, &journal).map_err(io_error)?;

        for file in &journal.files {
            if let Err(e) = replace(folder, &work, file) {
                // The folder was writable when the update was prepared: what refuses now is a
                // file another program keeps open (or a read-only file).
                let failure = if in_use(&e) || e.kind() == io::ErrorKind::PermissionDenied {
                    PortableFailure::InUse
                } else {
                    PortableFailure::Io
                };
                let detail = format!("{file}: {e}");
                drop(lock);
                return match undo(folder, &work, &journal) {
                    Ok(()) => Err(PortableError::new(failure, detail)),
                    Err(undo) => Err(PortableError::new(PortableFailure::Unrecovered, format!("{detail}; {undo}"))),
                };
            }
            replaced(file);
        }
        journal.stage = Stage::Committed;
        let committed = write_journal(&work, &journal);
        drop(lock);
        if committed.is_ok() {
            let _ = remove_tree(&work);
        }
        Ok(())
    }
}

/// One file: the current one is kept in `backup/`, the new one renamed over it.
fn replace(folder: &Path, work: &Path, file: &str) -> io::Result<()> {
    let target = below(folder, file);
    let new = below(&work.join(NEW), file);
    if target.is_dir() {
        return Err(io::Error::new(io::ErrorKind::AlreadyExists, "a folder has the file's name"));
    }
    if target.exists() {
        let backup = below(&work.join(BACKUP), file);
        if let Some(parent) = backup.parent() {
            fs::create_dir_all(parent)?;
        }
        if fs::hard_link(&target, &backup).is_err() {
            // FAT and exFAT drives have no hard links.
            fs::copy(&target, &backup)?;
        }
    } else if let Some(parent) = target.parent() {
        fs::create_dir_all(parent)?;
    }
    retrying(|| fs::rename(&new, &target))
}

/// Puts back every file the update replaced: a file still in `new/` was not replaced; a
/// replaced one is restored from `backup/`, or removed when it is new. Then the folders the
/// update created and the work folder are removed. The first failure is returned and the work
/// folder kept.
fn undo(folder: &Path, work: &Path, journal: &Journal) -> io::Result<()> {
    let mut first = Ok(());
    for file in journal.files.iter().rev() {
        if below(&work.join(NEW), file).exists() {
            continue;
        }
        let target = below(folder, file);
        let backup = below(&work.join(BACKUP), file);
        let result = if backup.exists() {
            retrying(|| fs::rename(&backup, &target))
        } else {
            match retrying(|| fs::remove_file(&target)) {
                Err(e) if e.kind() == io::ErrorKind::NotFound => Ok(()),
                other => other,
            }
        };
        if let Err(e) = result {
            if first.is_ok() {
                first = Err(io::Error::new(e.kind(), format!("{file}: {e}")));
            }
        }
    }
    first?;
    for dir in journal.created_dirs.iter().rev() {
        let _ = fs::remove_dir(below(folder, dir));
    }
    remove_tree(work)
}

/// At start: finishes or undoes an update a helper left unfinished, and removes the work folder
/// of one that was prepared but never applied or that is done. Leaves a helper's running update
/// alone.
pub fn recover(folder: &Path) -> Recovery {
    let work = work_dir(folder);
    if !work.is_dir() {
        return Recovery::None;
    }
    let lock = match lock(&work) {
        Ok(Some(lock)) => lock,
        Ok(None) => return Recovery::None,
        Err(e) => return Recovery::Failed(e.to_string()),
    };
    let journal = match read_journal(&work) {
        Ok(journal) if journal.stage == Stage::Applying => journal,
        _ => {
            drop(lock);
            let _ = remove_tree(&work);
            return Recovery::None;
        }
    };
    let finished = journal.files.iter().all(|f| !below(&work.join(NEW), f).exists());
    drop(lock);
    if finished {
        let _ = remove_tree(&work);
        return Recovery::Completed;
    }
    match undo(folder, &work, &journal) {
        Ok(()) => Recovery::RolledBack,
        Err(e) => Recovery::Failed(e.to_string()),
    }
}
