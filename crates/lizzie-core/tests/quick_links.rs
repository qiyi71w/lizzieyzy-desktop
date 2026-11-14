//! 快速启动 through the workbench operations the menu and 快速启动设置 use, and its Java import.
use std::path::Path;
use std::time::{Duration, Instant};

use lizzie_core::api::{CommandResult, ImportReason, ImportStatus, ImportTarget};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::quick_links::{QuickLinkProgram, QuickLinkSettings};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{Workbench, QUICK_LINK_FAILED};
use serde_json::json;

struct NoEngine;
impl Launcher for NoEngine {
    fn launch(&self, _: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        Err(std::io::Error::other("not started"))
    }
}

fn workbench(root: &Path) -> Workbench {
    Workbench::new(Storage::in_dirs(&root.join("config"), &root.join("data")), Some("zh-CN"), Box::new(NoEngine), Vec::new())
}

fn program(name: &str, command: &str) -> QuickLinkProgram {
    QuickLinkProgram { name: name.into(), command: command.into() }
}

#[test]
fn the_saved_list_drops_empty_commands_and_survives_a_restart() {
    let dir = tempfile::tempdir().unwrap();
    let mut wb = workbench(dir.path());
    assert_eq!(wb.view(false).settings.quick_links, QuickLinkSettings::default());
    let edited = QuickLinkSettings {
        show: true,
        programs: vec![program("A", "  /bin/a  "), program("deleted", " "), program("B", "/bin/b x")],
    };
    assert!(wb.set_quick_links(edited).unwrap());
    let expected = QuickLinkSettings { show: true, programs: vec![program("A", "/bin/a"), program("B", "/bin/b x")] };
    assert_eq!(wb.view(false).settings.quick_links, expected);
    drop(wb);
    assert_eq!(workbench(dir.path()).view(false).settings.quick_links, expected);
}

#[cfg(unix)]
#[test]
fn choosing_a_program_starts_it_with_its_arguments_and_a_failure_is_refused() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let tool = dir.path().join("tool.sh");
    std::fs::write(&tool, "#!/bin/sh\nprintf '%s\\n' \"$PWD\" \"$@\" > \"$(dirname \"$0\")/marker.txt\"\n").unwrap();
    std::fs::set_permissions(&tool, std::fs::Permissions::from_mode(0o755)).unwrap();
    let mut wb = workbench(dir.path());
    let command = format!("\"{}\" one \"two words\"", tool.display());
    wb.set_quick_links(QuickLinkSettings {
        show: true,
        programs: vec![program("Tool", &command), program("Missing", "/nonexistent/quick-link-tool")],
    })
    .unwrap();
    assert!(!dir.path().join("marker.txt").exists(), "saving starts nothing");

    assert_eq!(wb.launch_quick_link(0), CommandResult::Done);
    let marker = dir.path().join("marker.txt");
    let deadline = Instant::now() + Duration::from_secs(10);
    let written = loop {
        if let Ok(text) = std::fs::read_to_string(&marker) {
            if text.lines().count() == 3 {
                break text;
            }
        }
        assert!(Instant::now() < deadline, "the program did not run");
        std::thread::sleep(Duration::from_millis(20));
    };
    // The working directory is the program's folder, as for an engine given by absolute path.
    let cwd = dir.path().canonicalize().unwrap();
    assert_eq!(written.lines().collect::<Vec<_>>(), [cwd.to_str().unwrap(), "one", "two words"]);

    let refused = CommandResult::Refused { message_key: QUICK_LINK_FAILED.into() };
    assert_eq!(wb.launch_quick_link(1), refused);
    assert_eq!(wb.launch_quick_link(2), CommandResult::Unchanged);
}

fn java_dir(root: &Path, config: serde_json::Value) -> std::path::PathBuf {
    let java = root.join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    java
}

#[test]
fn java_programs_and_the_switch_come_over_without_starting_anything() {
    let dir = tempfile::tempdir().unwrap();
    let marker = dir.path().join("started");
    let java = java_dir(
        dir.path(),
        json!({
            "ui": {"show-quick-links": true},
            // Java skips an empty command and names each program by the name at its index.
            "leelaz": {
                "program-name-list": ["Notes", "gone", "Touch"],
                "program-command-list": ["/usr/bin/true", "", format!("/usr/bin/touch {}", marker.display())],
            },
        }),
    );
    let mut wb = workbench(dir.path());
    let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    assert!(!p.quick_link_credential);
    for key in ["ui.show-quick-links", "leelaz.program-name-list", "leelaz.program-command-list"] {
        let it = p.items.iter().find(|i| i.key == key).unwrap();
        assert_eq!((it.status, it.target), (ImportStatus::Imported, Some(ImportTarget::QuickLinks)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let expected = QuickLinkSettings {
        show: true,
        programs: vec![program("Notes", "/usr/bin/true"), program("Touch", &format!("/usr/bin/touch {}", marker.display()))],
    };
    assert_eq!(wb.view(false).settings.quick_links, expected);
    std::thread::sleep(Duration::from_millis(200));
    assert!(!marker.exists(), "importing starts no program");
    assert_eq!(workbench(dir.path()).view(false).settings.quick_links, expected);
}

#[test]
fn a_command_with_credentials_comes_over_only_when_confirmed() {
    let config = json!({
        "ui": {"show-quick-links": true},
        "leelaz": {"program-name-list": ["Plain", "Secret"], "program-command-list": ["/bin/plain", "/bin/tool --password=s3cret"]},
    });
    for confirmed in [false, true] {
        let dir = tempfile::tempdir().unwrap();
        let java = java_dir(dir.path(), config.clone());
        let mut wb = workbench(dir.path());
        let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
        assert!(p.quick_link_credential);
        let shown = p.items.iter().find(|i| i.key == "leelaz.program-command-list").unwrap();
        assert!(!shown.value.as_deref().unwrap().contains("s3cret"), "the preview never shows the secret");
        let report = wb.apply_java_import(p.id, &[], false, confirmed, true, None).unwrap().unwrap();
        for key in ["leelaz.program-name-list", "leelaz.program-command-list"] {
            let it = report.items.iter().find(|i| i.key == key).unwrap();
            let expected = if confirmed { (ImportStatus::Imported, None) } else { (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed)) };
            assert_eq!((it.status, it.reason), expected, "{key} confirmed={confirmed}");
        }
        let settings = wb.view(false).settings.quick_links;
        assert!(settings.show, "the switch comes over either way");
        let commands: Vec<_> = settings.programs.iter().map(|p| p.command.as_str()).collect();
        if confirmed {
            assert_eq!(commands, ["/bin/plain", "/bin/tool --password=s3cret"]);
        } else {
            assert!(commands.is_empty());
        }
    }
}

#[test]
fn a_command_list_java_cannot_read_is_refused_whole() {
    let dir = tempfile::tempdir().unwrap();
    let java = java_dir(dir.path(), json!({"leelaz": {"program-name-list": ["A", "B"], "program-command-list": ["/bin/a", 3]}}));
    let mut wb = workbench(dir.path());
    let p = wb.preview_java_import(java_import::read(&JavaSource::new(&java, None)).unwrap());
    for key in ["leelaz.program-name-list", "leelaz.program-command-list"] {
        let it = p.items.iter().find(|i| i.key == key).unwrap();
        assert_eq!((it.status, it.reason), (ImportStatus::Incompatible, Some(ImportReason::InvalidValue)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(wb.view(false).settings.quick_links, QuickLinkSettings::default());
}
