//! The portable zip's file replacement through the operations the program and its helper use:
//! prepare while the program runs, claim and apply after it closed, recover at the next start.
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use lizzie_core::update::portable::{self, PortableFailure, Recovery, WORK_DIR_NAME};

#[cfg(unix)]
#[path = "support/permissions.rs"]
mod permissions;
#[cfg(unix)]
use permissions::obeying_permissions;

const BINARY: &str = "lizzieyzy-tauri.exe";

/// A release zip: one folder holding the given files.
fn package(dir: &Path, files: &[(&str, &str)]) -> PathBuf {
    let path = dir.join("package.zip");
    let mut zip = zip::ZipWriter::new(fs::File::create(&path).unwrap());
    for (name, text) in files {
        zip.start_file(format!("LizzieYzy Tauri/{name}"), zip::write::SimpleFileOptions::default())
            .unwrap();
        zip.write_all(text.as_bytes()).unwrap();
    }
    zip.finish().unwrap();
    path
}

/// A portable folder of version 1 whose program was renamed by the user, with ReadBoard's own
/// settings and a file of the user's.
fn folder(root: &Path) -> PathBuf {
    let folder = root.join("portable");
    fs::create_dir_all(folder.join("readboard")).unwrap();
    fs::write(folder.join("LizzieYzy.exe"), "program 1").unwrap();
    fs::write(folder.join("LICENSE.txt"), "licence 1").unwrap();
    fs::write(folder.join("readboard/readboard.exe"), "readboard 1").unwrap();
    fs::write(folder.join("readboard/config.readboard.json"), "user readboard settings").unwrap();
    fs::write(folder.join("my-notes.txt"), "user file").unwrap();
    folder
}

fn read(path: impl AsRef<Path>) -> String {
    fs::read_to_string(path).unwrap()
}

const VERSION_2: &[(&str, &str)] = &[
    ("LICENSE.txt", "licence 2"),
    ("readboard/readboard.exe", "readboard 2"),
    ("readboard/runtimes/new.dll", "new 2"),
    (BINARY, "program 2"),
];

fn snapshot(folder: &Path) -> Vec<(String, String)> {
    let mut files = Vec::new();
    let mut stack = vec![folder.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in fs::read_dir(dir).unwrap().flatten() {
            let path = entry.path();
            if path.is_dir() {
                files.push((path.strip_prefix(folder).unwrap().display().to_string(), "<dir>".into()));
                stack.push(path);
            } else {
                files.push((path.strip_prefix(folder).unwrap().display().to_string(), read(&path)));
            }
        }
    }
    files.sort();
    files
}

#[test]
fn an_update_replaces_only_the_packages_files_and_keeps_the_portable_folder() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let zip = package(root.path(), VERSION_2);

    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();
    // Preparing changes nothing the program uses.
    assert_eq!(read(folder.join("LizzieYzy.exe")), "program 1");
    let claim = portable::claim(&folder).unwrap();
    assert_eq!(claim.version(), "0.2.0");
    let mut replaced = Vec::new();
    claim.apply(&mut |f| replaced.push(f.to_string())).unwrap();

    // The renamed program keeps its name; the program is replaced last.
    assert_eq!(replaced.last().map(String::as_str), Some("LizzieYzy.exe"));
    assert_eq!(
        snapshot(&folder),
        [
            ("LICENSE.txt", "licence 2"),
            ("LizzieYzy.exe", "program 2"),
            ("my-notes.txt", "user file"),
            ("readboard", "<dir>"),
            ("readboard/config.readboard.json", "user readboard settings"),
            ("readboard/readboard.exe", "readboard 2"),
            ("readboard/runtimes", "<dir>"),
            ("readboard/runtimes/new.dll", "new 2"),
        ]
        .map(|(a, b)| (a.to_string(), b.to_string()))
    );
    assert_eq!(portable::recover(&folder), Recovery::None);
}

#[test]
fn a_package_that_is_not_this_programs_folder_is_refused_before_anything_changes() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let before = snapshot(&folder);
    let cases: Vec<Vec<(&str, &str)>> = vec![
        vec![("LICENSE.txt", "no program")],
        vec![(BINARY, "p"), ("../escape.txt", "x")],
        vec![(BINARY, "p"), (".lizzieyzy-update/journal.json", "x")],
        vec![(BINARY, "p"), ("LizzieYzy.exe", "would overwrite the renamed program twice")],
    ];
    for files in cases {
        let zip = package(root.path(), &files);
        let error = portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap_err();
        assert_eq!(error.failure, PortableFailure::Package, "{files:?}");
        assert_eq!(snapshot(&folder), before, "{files:?}");
    }
    // Not a zip at all.
    let junk = root.path().join("junk.zip");
    fs::write(&junk, "not a zip").unwrap();
    let error = portable::prepare(&junk, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap_err();
    assert_eq!(error.failure, PortableFailure::Package);
    assert_eq!(snapshot(&folder), before);
}

#[cfg(unix)]
fn set_mode(path: &Path, mode: u32) {
    use std::os::unix::fs::PermissionsExt;
    fs::set_permissions(path, fs::Permissions::from_mode(mode)).unwrap();
}

#[cfg(unix)]
#[test]
fn an_unwritable_folder_is_reported_while_the_program_still_runs() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let zip = package(root.path(), VERSION_2);
    let before = snapshot(&folder);
    set_mode(&folder, 0o555);
    let result = obeying_permissions(|| portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0"));
    set_mode(&folder, 0o755);
    assert_eq!(result.unwrap_err().failure, PortableFailure::Permission);
    assert_eq!(snapshot(&folder), before);
}

#[cfg(unix)]
#[test]
fn a_file_that_cannot_be_replaced_puts_every_replaced_file_back() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    fs::create_dir(folder.join("locked")).unwrap();
    fs::write(folder.join("locked/held.txt"), "held 1").unwrap();
    let mut files = VERSION_2.to_vec();
    files.insert(3, ("locked/held.txt", "held 2"));
    let zip = package(root.path(), &files);
    let before = snapshot(&folder);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();

    // The file cannot be replaced after the program closed (Windows: kept open elsewhere).
    set_mode(&folder.join("locked"), 0o555);
    let mut replaced = Vec::new();
    let result = obeying_permissions(|| portable::claim(&folder).unwrap().apply(&mut |f| replaced.push(f.to_string())));
    set_mode(&folder.join("locked"), 0o755);

    let error = result.unwrap_err();
    assert_eq!(error.failure, PortableFailure::InUse, "{error}");
    assert!(replaced.len() >= 2, "{replaced:?}");
    // Everything is as before, including the folder the update had created; nothing is left.
    assert_eq!(snapshot(&folder), before);
    assert!(!folder.join(WORK_DIR_NAME).exists());
}

/// Runs the helper's replacement and stops it after `files` files, as a power cut would.
fn interrupted_after(folder: &Path, files: usize) {
    let claim = portable::claim(folder).unwrap();
    let mut count = 0;
    let stopped = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| {
        claim.apply(&mut |_| {
            count += 1;
            if count == files {
                panic!("stopped");
            }
        })
    }));
    assert!(stopped.is_err());
}

#[test]
fn an_interrupted_update_is_undone_at_the_next_start() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let before = snapshot(&folder);
    let zip = package(root.path(), VERSION_2);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();
    interrupted_after(&folder, 2);
    assert_eq!(read(folder.join("LICENSE.txt")), "licence 2");

    assert_eq!(portable::recover(&folder), Recovery::RolledBack);
    assert_eq!(snapshot(&folder), before);
}

#[cfg(unix)]
#[test]
fn undoing_again_after_a_partly_failed_undo_keeps_the_files_already_put_back() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let before = snapshot(&folder);
    let zip = package(root.path(), VERSION_2);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();
    interrupted_after(&folder, 3);

    // One file cannot be put back at this start; the others are.
    set_mode(&folder.join("readboard"), 0o555);
    let first = obeying_permissions(|| portable::recover(&folder));
    set_mode(&folder.join("readboard"), 0o755);
    assert!(matches!(first, Recovery::Failed(_)), "{first:?}");
    assert_eq!(read(folder.join("LICENSE.txt")), "licence 1");

    // The next start finishes putting them back without losing the ones already restored.
    assert_eq!(portable::recover(&folder), Recovery::RolledBack);
    assert_eq!(snapshot(&folder), before);
    assert_eq!(portable::recover(&folder), Recovery::None);
}

#[test]
fn an_update_interrupted_after_its_last_file_is_finished_at_the_next_start() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let zip = package(root.path(), VERSION_2);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();
    interrupted_after(&folder, VERSION_2.len());

    assert_eq!(portable::recover(&folder), Recovery::Completed);
    assert_eq!(read(folder.join("LizzieYzy.exe")), "program 2");
    assert_eq!(read(folder.join("readboard/runtimes/new.dll")), "new 2");
    assert!(!folder.join(WORK_DIR_NAME).exists());
}

#[test]
fn a_program_started_while_the_helper_waits_leaves_the_update_alone() {
    let root = tempfile::tempdir().unwrap();
    let folder = folder(root.path());
    let zip = package(root.path(), VERSION_2);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.2.0").unwrap();
    let claim = portable::claim(&folder).unwrap();
    assert_eq!(portable::recover(&folder), Recovery::None);
    claim.apply(&mut |_| {}).unwrap();
    assert_eq!(read(folder.join("LizzieYzy.exe")), "program 2");

    // A prepared update nobody claims (the helper never ran) is removed at the next start.
    let zip = package(root.path(), &[(BINARY, "program 3")]);
    portable::prepare(&zip, &folder, "LizzieYzy.exe", BINARY, "0.3.0").unwrap();
    assert_eq!(portable::recover(&folder), Recovery::None);
    assert!(!folder.join(WORK_DIR_NAME).exists());
    assert_eq!(read(folder.join("LizzieYzy.exe")), "program 2");
}
