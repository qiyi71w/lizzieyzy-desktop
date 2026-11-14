//! The deb handover's decisions, from the text `dpkg-deb`, `dpkg-query` and `apt-get` print: which
//! package is this installation's update, and what the system package manager did with it.
use lizzie_core::update::deb::{reasons, settle, Handover, Package};

/// `dpkg-query --status <package>` of the installed program.
fn installed(version: &str, status: &str) -> Package {
    Package::parse(&format!(
        "Package: lizzie-yzy-tauri\nStatus: {status}\nPriority: optional\nInstalled-Size: 47353\n\
         Maintainer: qiyi71w\nArchitecture: amd64\nVersion: {version}\n\
         Depends: libwebkit2gtk-4.1-0, libgtk-3-0\nDescription: Go (weiqi) game review workbench\n"
    ))
    .unwrap()
}

/// `dpkg-deb --field <file> Package Version Architecture` of a downloaded package.
fn file(name: &str, version: &str, architecture: &str) -> Package {
    Package::parse(&format!(
        "Package: {name}\nVersion: {version}\nArchitecture: {architecture}\n"
    ))
    .unwrap()
}

const OK: &str = "install ok installed";

#[test]
fn only_the_offered_version_of_this_package_and_architecture_is_handed_over() {
    let current = installed("0.1.0", OK);
    assert!(file("lizzie-yzy-tauri", "0.2.0", "amd64").updates(&current, "0.2.0"));
    assert!(file("lizzie-yzy-tauri", "0.2.0", "all").updates(&current, "0.2.0"));
    for (other, why) in [
        (
            file("lizzie-yzy-tauri-acceptance", "0.2.0", "amd64"),
            "another package",
        ),
        (file("lizzie-yzy-tauri", "0.2.0", "arm64"), "another architecture"),
        (
            file("lizzie-yzy-tauri", "0.3.0", "amd64"),
            "not the version that was offered",
        ),
    ] {
        assert!(!other.updates(&current, "0.2.0"), "{why}");
    }
    // Output that is not a package's fields is not a package.
    assert_eq!(
        Package::parse("dpkg-deb: error: 'x.deb' is not a Debian format archive\n"),
        None
    );
}

#[test]
fn a_handover_is_settled_by_what_the_package_manager_recorded() {
    let old = installed("0.1.0", OK);
    let new = installed("0.2.0", OK);
    for (exit, now, expected, why) in [
        (Some(0), Some(&new), Handover::Installed, "installed"),
        // A clean exit that left the old version is not an installation.
        (Some(0), Some(&old), Handover::NotInstalled, "exit 0, nothing installed"),
        // apt can end with an error from another package after this one was installed.
        (Some(100), Some(&new), Handover::Installed, "installed despite an error"),
        (Some(126), Some(&old), Handover::Dismissed, "prompt dismissed"),
        (Some(127), Some(&old), Handover::Unauthorized, "not authorized"),
        (
            Some(100),
            Some(&old),
            Handover::NotInstalled,
            "apt refused: dependency, lock, architecture",
        ),
        (None, Some(&old), Handover::NotInstalled, "ended by a signal before installing"),
        (None, Some(&new), Handover::Installed, "the program closed meanwhile"),
    ] {
        assert_eq!(settle(exit, now, "0.2.0"), expected, "{why}");
    }
    // Stopped part-way: dpkg's record is not an installed state, whatever the exit code.
    for status in [
        "install reinstreq half-installed",
        "install ok unpacked",
        "install ok half-configured",
        "deinstall ok config-files",
    ] {
        for version in ["0.1.0", "0.2.0"] {
            let now = installed(version, status);
            for exit in [Some(0), Some(100), None] {
                assert_eq!(
                    settle(exit, Some(&now), "0.2.0"),
                    Handover::Unfinished,
                    "{status} {version}"
                );
            }
        }
    }
    assert_eq!(settle(Some(100), None, "0.2.0"), Handover::Unfinished, "no record at all");
}

#[test]
fn the_package_managers_own_reasons_are_its_error_lines() {
    let locked = "Reading package lists...\nBuilding dependency tree...\n\
                  E: Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 4242 (apt-get)\n\
                  N: Be aware that removing the lock file is not a solution\n\
                  E: Unable to acquire the dpkg frontend lock, is another process using it?\n";
    assert_eq!(
        reasons(locked).as_deref(),
        Some(
            "Could not get lock /var/lib/dpkg/lock-frontend. It is held by process 4242 (apt-get)\n\
             Unable to acquire the dpkg frontend lock, is another process using it?"
        )
    );
    // A dependency apt cannot install: the line that names it comes with apt's error.
    let unmet = "Reading state information...\n\
                 Some packages could not be installed. This may mean that you have\n\
                 requested an impossible situation.\n\
                 The following information may help to resolve the situation:\n\n\
                 The following packages have unmet dependencies:\n \
                 lizzie-yzy-tauri : Depends: libexample1 but it is not installable\n\
                 E: Unable to correct problems, you have held broken packages.\n";
    assert_eq!(
        reasons(unmet).as_deref(),
        Some(
            "lizzie-yzy-tauri : Depends: libexample1 but it is not installable\n\
             Unable to correct problems, you have held broken packages."
        )
    );
    // Without an error line, its last word.
    assert_eq!(
        reasons("Reading package lists...\nlizzie-yzy-tauri is already the newest version (0.1.0).\n").as_deref(),
        Some("lizzie-yzy-tauri is already the newest version (0.1.0).")
    );
    assert_eq!(reasons("\n  \n"), None);
}
