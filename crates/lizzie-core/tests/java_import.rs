//! Importing the fixed Java program's settings through the public workbench operations: the
//! Java files (`config.txt`, `persist`, written the way `Config.java` writes them) are only read;
//! this product's settings change only on a confirmed, complete import; credentials are copied
//! only when confirmed and never appear in the preview or report.

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lizzie_core::api::{EngineProblem, ImportFailure, ImportReason, ImportStatus, ImportTarget, JavaImportView};
use lizzie_core::engine::gtp::{split_command, LaunchSpec};
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::settings::{Config, Language, Persisted, Storage};
use lizzie_core::workbench::{Workbench, IMPORT_NEEDS_REPLACE, IMPORT_STALE};
use serde_json::json;

/// Records what would be launched; no process is started.
#[derive(Clone, Default)]
struct Recorder(Arc<Mutex<Vec<LaunchSpec>>>);

impl Launcher for Recorder {
    fn launch(&self, _: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0.lock().unwrap().push(spec.clone());
        Err(std::io::Error::other("not started in tests"))
    }
}

fn storage(dir: &Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

fn workbench(dir: &Path, launcher: Recorder) -> Workbench {
    Workbench::new(storage(dir), Some("zh-CN"), Box::new(launcher), Vec::new())
}

const SECRETS: [&str; 3] = ["UPLOAD-SECRET", "SSH-SECRET", "TOKEN-SECRET"];

/// A Java work directory (with a space in its name) holding the files one Java profile has.
fn java_dir(root: &Path) -> PathBuf {
    let dir = root.join("Lizzie Next");
    for f in ["engines/fake-katago", "weights/model.bin.gz", "cfg/gtp.cfg"] {
        std::fs::create_dir_all(dir.join(f).parent().unwrap()).unwrap();
        std::fs::write(dir.join(f), "x").unwrap();
    }
    std::fs::create_dir_all(dir.join("save")).unwrap();
    let recent: Vec<_> = (1..=6).map(|i| json!({ "path": format!("/games/{i}.sgf") })).collect();
    let config = json!({
        "ui": {
            "read-komi": false,
            "load-sgf-last": true,
            "show-replace-file-hint": false,
            "play-sound": false,
            "use-language": 2,
            "limit-time": false,
            "limit-playout": "true",
            "limit-playouts": 5000,
            "show-ponder-limited-tips": false,
            "kata-rules": "japanese",
            "auto-load-kata-rules": true,
            "recent-file-paths": recent,
            "autoload-default": false,
            "autoload-last": true,
            "autoload-empty": false,
            "default-engine": 0,
            "last-engine": 0,
            "board-theme": "wood",
            "up-load-passwd": SECRETS[0],
        },
        "leelaz": {
            "max-analyze-time-seconds": 30,
            "limit-max-suggestion": -3,
            "limit-branch-length": 8,
            "analyze-update-interval-centisec": 20,
            "fast-engine-change": false,
            "engine-settings-list": [
                {
                    "id": "java-0",
                    "name": "Local",
                    "command": "engines/fake-katago gtp -model weights/model.bin.gz -config cfg/gtp.cfg",
                    "initialCommand": "time_settings 0 1 0",
                    "isDefault": true,
                    "preload": false,
                    "komi": 6.5,
                    "width": 19,
                    "height": 19,
                    "useJavaSSH": false,
                    "ip": "", "port": "", "userName": "", "password": "", "useKeyGen": false, "keyGenPath": "",
                    "threadPolicy": { "source": "CFG" },
                },
                {
                    "id": "java-1",
                    "name": "Broken",
                    "command": "/nonexistent/katago gtp -model /nonexistent/model.bin.gz",
                    "width": 9,
                    "height": 99,
                },
                {
                    "id": "java-2",
                    "name": "Remote",
                    "command": "katago gtp",
                    "useJavaSSH": true,
                    "ip": "10.0.0.1",
                    "userName": "go",
                    "password": SECRETS[1],
                },
                {
                    "id": "java-3",
                    "name": "Proxy",
                    "command": format!("engines/fake-katago --token {}", SECRETS[2]),
                },
            ],
        },
        "logging": { "diagnosticsEnabled": false },
    });
    std::fs::write(dir.join("config.txt"), serde_json::to_string_pretty(&config).unwrap()).unwrap();
    let persist = json!({
        "filesystem": { "last-folder": "/games", "last-image-folder": "/images" },
        "ui-persist": { "main-window-position": [10, 20, 1500, 900], "window-maximized": false },
        "autosave": {},
    });
    std::fs::write(dir.join("persist"), serde_json::to_string_pretty(&persist).unwrap()).unwrap();
    dir
}

/// Every file under `dir` with its bytes, to show the Java folder is untouched.
fn snapshot(dir: &Path) -> Vec<(PathBuf, Vec<u8>)> {
    let mut out = Vec::new();
    let mut stack = vec![dir.to_path_buf()];
    while let Some(d) = stack.pop() {
        for e in std::fs::read_dir(&d).unwrap() {
            let p = e.unwrap().path();
            if p.is_dir() {
                out.push((p.clone(), Vec::new()));
                stack.push(p);
            } else {
                out.push((p.clone(), std::fs::read(&p).unwrap()));
            }
        }
    }
    out.sort();
    out
}

fn preview(wb: &mut Workbench, dir: &Path) -> lizzie_core::api::ImportPreviewView {
    let candidate = java_import::read(&JavaSource::new(dir, None)).unwrap();
    wb.preview_java_import(candidate)
}

fn item<'a>(items: &'a [lizzie_core::api::ImportItemView], key: &str) -> &'a lizzie_core::api::ImportItemView {
    items
        .iter()
        .find(|i| i.key == key)
        .unwrap_or_else(|| panic!("no item {key}"))
}

fn saved_config(dir: &Path) -> Config {
    serde_json::from_slice(&std::fs::read(storage(dir).config_file).unwrap()).unwrap()
}

#[test]
fn imports_basic_settings_and_reports_every_other_java_setting() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let before = snapshot(&java);
    let product = tmp.path().join("product");
    let recorder = Recorder::default();
    let mut wb = workbench(&product, recorder.clone());

    let p = preview(&mut wb, &java);
    let shown = serde_json::to_string(&p).unwrap();
    for secret in SECRETS {
        assert!(!shown.contains(secret), "{secret} must not appear in the preview");
    }
    assert!(p.replaces.is_empty(), "nothing chosen in this product yet");
    let status = |key: &str| item(&p.items, key).status;
    assert_eq!(status("ui.read-komi"), ImportStatus::Imported);
    assert_eq!(item(&p.items, "ui.limit-playout").value.as_deref(), Some("true"));
    assert_eq!(status("leelaz.limit-max-suggestion"), ImportStatus::Incompatible);
    assert_eq!(status("ui.board-theme"), ImportStatus::NotMigrated);
    let upload = item(&p.items, "ui.up-load-passwd");
    assert_eq!(
        (upload.status, upload.reason, upload.credential),
        (ImportStatus::NotMigrated, Some(ImportReason::Credential), true)
    );
    assert_eq!(status("logging.diagnosticsEnabled"), ImportStatus::NotMigrated);
    assert_eq!(status("persist:filesystem.last-folder"), ImportStatus::Imported);
    assert_eq!(
        status("persist:filesystem.last-image-folder"),
        ImportStatus::NotMigrated
    );
    let window = item(&p.items, "persist:ui-persist.main-window-position");
    assert_eq!(
        (window.status, window.reason),
        (ImportStatus::Incompatible, Some(ImportReason::WindowCoordinates))
    );
    assert_eq!(status("file:save"), ImportStatus::NotMigrated);

    // Engines: Java-relative paths resolved against the Java folder; problems found without
    // starting anything; the remote engine left out; the one with a token needs confirmation.
    let local = &p.engines[0];
    let tokens = split_command(&local.command);
    assert_eq!(tokens[0], java.join("engines/fake-katago").to_string_lossy());
    assert_eq!(tokens[3], java.join("weights/model.bin.gz").to_string_lossy());
    assert!(local.problems.is_empty() && local.is_default && local.unmigrated_fields == ["threadPolicy"]);
    assert!(p.engines[1].problems.contains(&EngineProblem::ProgramNotFound {
        path: "/nonexistent/katago".into()
    }));
    assert!(p.engines[1].problems.contains(&EngineProblem::FileNotFound {
        option: "-model".into(),
        path: "/nonexistent/model.bin.gz".into()
    }));
    assert!(p.engines[1]
        .problems
        .contains(&EngineProblem::InvalidValue { field: "height".into() }));
    assert_eq!(
        (p.engines[2].status, p.engines[2].reason),
        (ImportStatus::NotMigrated, Some(ImportReason::RemoteEngine))
    );
    assert!(p.engines[3].credential);

    let report = wb.apply_java_import(p.id, &[], false).unwrap().unwrap();
    assert!(report.restart_required, "English applies after a restart");
    assert_eq!(
        (report.engines[3].status, report.engines[3].reason),
        (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed))
    );
    let shown = serde_json::to_string(&report).unwrap();
    assert!(SECRETS.iter().all(|s| !shown.contains(s)));

    let c = saved_config(&product);
    assert_eq!(c.ui.language, Language::EnUs);
    assert!(!c.record.read_komi && c.record.load_sgf_last && !c.record.show_replace_file_hint && !c.ui.play_sound);
    assert_eq!(
        c.recent_files,
        (2..=6).map(|i| format!("/games/{i}.sgf")).collect::<Vec<_>>()
    );
    let a = &c.engine.analysis;
    assert_eq!(
        (
            a.limit_time,
            a.limit_playout,
            a.limit_playouts,
            a.max_analyze_time_seconds
        ),
        (false, true, 5000, 30)
    );
    assert_eq!(
        (a.limit_max_suggestion, a.limit_branch_length, a.interval_centisec),
        (10, 8, 20)
    );
    assert!(!a.fast_engine_change && !a.show_ponder_limited_tips);
    assert_eq!(
        (c.engine.kata_rules.as_str(), c.engine.auto_load_kata_rules),
        ("japanese", true)
    );
    let names: Vec<&str> = c.engine.engines.iter().map(|e| e.name.as_str()).collect();
    assert_eq!(names, ["Local", "Broken"]);
    let local = &c.engine.engines[0];
    assert_eq!(
        (local.komi, local.initial_command.as_str(), local.is_default),
        (6.5, "time_settings 0 1 0", true)
    );
    assert_eq!((c.engine.engines[1].width, c.engine.engines[1].height), (9, 19));
    assert_eq!(
        (c.engine.startup, c.engine.last_engine.as_deref()),
        (EngineStartup::Last, Some("java-0"))
    );
    let persisted: Persisted = serde_json::from_slice(&std::fs::read(storage(&product).persist_file).unwrap()).unwrap();
    assert_eq!(persisted.last_folder.as_deref(), Some("/games"));
    assert!(!std::fs::read_to_string(storage(&product).config_file)
        .unwrap()
        .contains("SECRET"));

    // The Java folder is exactly as it was.
    assert_eq!(snapshot(&java), before);

    // The imported engine starts at once and after a restart, from the Java engine's files.
    let launched = recorder.0.lock().unwrap().clone();
    assert_eq!(
        launched.last().unwrap().program,
        java.join("engines/fake-katago").to_string_lossy()
    );
    let again = Recorder::default();
    let mut wb2 = workbench(&product, again.clone());
    wb2.start_engines();
    assert_eq!(
        again.0.lock().unwrap()[0].program,
        java.join("engines/fake-katago").to_string_lossy()
    );
    assert_eq!(wb2.view(false).settings.active_language, Language::EnUs);
}

#[test]
fn credentials_are_copied_only_when_confirmed() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    let report = wb.apply_java_import(p.id, &[3], false).unwrap().unwrap();
    assert_eq!(report.engines[3].status, ImportStatus::Imported);
    let c = saved_config(&product);
    let proxy = c.engine.engines.iter().find(|e| e.name == "Proxy").unwrap();
    assert!(
        proxy.command.ends_with(SECRETS[2]),
        "the confirmed command is kept as written"
    );
    // A preview is applied once.
    assert_eq!(wb.apply_java_import(p.id, &[], false).unwrap(), Err(IMPORT_STALE));
}

#[test]
fn settings_chosen_in_this_product_are_replaced_only_after_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let mine = EngineProfile {
        name: "Mine".into(),
        command: "mine gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![mine], EngineStartup::Empty).unwrap();
    wb.set_play_sound(false).unwrap();
    let before = std::fs::read(storage(&product).config_file).unwrap();

    let p = preview(&mut wb, &java);
    assert!(p.replaces.contains(&ImportTarget::Engines));
    assert!(p.replaces.contains(&ImportTarget::EngineStartup));
    assert!(
        !p.replaces.contains(&ImportTarget::PlaySound),
        "same value as Java: nothing to replace"
    );
    assert_eq!(
        wb.apply_java_import(p.id, &[], false).unwrap(),
        Err(IMPORT_NEEDS_REPLACE)
    );
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), before);
    assert_eq!(wb.config().engine.engines[0].name, "Mine");

    wb.apply_java_import(p.id, &[], true).unwrap().unwrap();
    assert_eq!(wb.config().engine.engines[0].name, "Local");
}

#[test]
fn a_failed_write_leaves_both_settings_files_and_the_session_unchanged() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.set_play_sound(false).unwrap();
    let config_before = std::fs::read(storage(&product).config_file).unwrap();
    // persist.json cannot be replaced: a directory is in its place.
    std::fs::create_dir_all(storage(&product).persist_file.join("blocker")).unwrap();

    let p = preview(&mut wb, &java);
    assert!(wb.apply_java_import(p.id, &[], true).is_err());
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), config_before);
    assert_eq!(wb.config().ui.language, Language::System);
    assert!(wb.config().engine.engines.is_empty());
    let leftovers: Vec<_> = std::fs::read_dir(product.join("config"))
        .unwrap()
        .map(|e| e.unwrap().file_name().to_string_lossy().into_owned())
        .filter(|n| n.ends_with(".tmp"))
        .collect();
    assert!(leftovers.is_empty(), "{leftovers:?}");

    // Once the obstacle is gone the same preview imports.
    std::fs::remove_dir_all(storage(&product).persist_file).unwrap();
    wb.apply_java_import(p.id, &[], true).unwrap().unwrap();
    assert_eq!(wb.config().ui.language, Language::EnUs);
}

#[cfg(unix)]
#[test]
fn a_settings_file_that_cannot_be_kept_is_not_replaced() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.set_play_sound(false).unwrap();
    let config_file = storage(&product).config_file;
    let before = std::fs::read(&config_file).unwrap();
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(0o000)).unwrap();
    if std::fs::read(&config_file).is_ok() {
        return; // running with privileges that read anything: nothing to show
    }
    std::fs::create_dir_all(storage(&product).persist_file.join("blocker")).unwrap();
    let p = preview(&mut wb, &java);
    assert!(wb.apply_java_import(p.id, &[], true).is_err());
    std::fs::set_permissions(&config_file, std::fs::Permissions::from_mode(0o600)).unwrap();
    assert_eq!(
        std::fs::read(&config_file).unwrap(),
        before,
        "the existing settings file is kept"
    );
}

#[test]
fn diagnostics_and_rejected_values_never_show_credentials() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "kata-rules": "japanese", "recent-file-paths": [{ "path": "https://bob:RECENT-SECRET@example.invalid/a.sgf" }] },
        "leelaz": { "engine-settings-list": [{
            "name": "Missing",
            "command": "/missing/katago gtp -model https://alice:URL-SECRET@example.invalid/m.bin.gz",
        }] },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert!(!p.engines[0].problems.is_empty());
    let shown = serde_json::to_string(&p).unwrap();
    assert!(
        !shown.contains("URL-SECRET") && !shown.contains("RECENT-SECRET"),
        "{shown}"
    );

    // A malformed engine list is reported without its content.
    let broken = json!({ "leelaz": { "engine-settings-list": { "password": "OBJECT-SECRET" } } });
    std::fs::write(java.join("config.txt"), broken.to_string()).unwrap();
    let p = preview(&mut wb, &java);
    let list = item(&p.items, "leelaz.engine-settings-list");
    assert_eq!(list.status, ImportStatus::Incompatible);
    assert!(!serde_json::to_string(&p).unwrap().contains("OBJECT-SECRET"));
}

#[test]
fn once_this_product_has_settings_every_change_needs_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    // Chosen explicitly, even though it equals the default.
    wb.set_play_sound(false).unwrap();
    wb.set_play_sound(true).unwrap();
    let p = preview(&mut wb, &java);
    assert!(p.replaces.contains(&ImportTarget::PlaySound), "{:?}", p.replaces);
    assert_eq!(
        wb.apply_java_import(p.id, &[], false).unwrap(),
        Err(IMPORT_NEEDS_REPLACE)
    );
    assert!(wb.config().ui.play_sound);
}

#[test]
fn engine_paths_resolve_like_java_launches_them() {
    // `CommandLaunchHelper`: a bare program name is looked up on PATH, its relative resources
    // in the Java folder; a Java engine's relative `-jar` file is resolved too.
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    std::fs::write(java.join("engines/leela.jar"), "x").unwrap();
    let config = json!({ "leelaz": { "engine-settings-list": [
        { "name": "OnPath", "command": "katago-on-path gtp -model weights/model.bin.gz" },
        { "name": "Jar", "command": "java -jar engines/leela.jar gtp" },
    ] } });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let c = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let engines: Vec<_> = c
        .mapped
        .engines
        .unwrap()
        .into_iter()
        .map(|m| m.profile.command)
        .collect();
    let model = java.join("weights/model.bin.gz").to_string_lossy().into_owned();
    let jar = java.join("engines/leela.jar").to_string_lossy().into_owned();
    assert_eq!(
        split_command(&engines[0]),
        ["katago-on-path", "gtp", "-model", model.as_str()]
    );
    assert_eq!(split_command(&engines[1]), ["java", "-jar", jar.as_str(), "gtp"]);
}

#[test]
fn the_imported_last_folder_is_kept_after_resetting_the_interface_position() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.reset_ui_position().unwrap();
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], true).unwrap().unwrap();
    let reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.persisted().last_folder.as_deref(), Some("/games"));
}

#[test]
fn startup_choices_of_left_out_engines_are_reported_as_left_out() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "autoload-last": true, "last-engine": 1, "default-engine": 0 },
        "leelaz": { "engine-settings-list": [
            { "name": "Proxy", "command": "katago gtp --token TOKEN-SECRET", "isDefault": true },
            { "name": "Remote", "command": "katago gtp", "useJavaSSH": true },
        ] },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    let last = item(&p.items, "ui.last-engine");
    assert_eq!(
        (last.status, last.reason),
        (ImportStatus::NotMigrated, Some(ImportReason::RemoteEngine))
    );
    let report = wb.apply_java_import(p.id, &[], false).unwrap().unwrap();
    let default = item(&report.items, "ui.default-engine");
    assert_eq!(
        (default.status, default.reason),
        (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed))
    );
    assert!(wb.config().engine.engines.is_empty() && wb.config().engine.last_engine.is_none());
}

#[test]
fn unreadable_java_files_are_diagnosed_without_changing_anything() {
    let tmp = tempfile::tempdir().unwrap();
    let empty = tmp.path().join("empty");
    std::fs::create_dir_all(&empty).unwrap();
    assert!(matches!(
        java_import::read(&JavaSource::new(&empty, None)),
        Err(ImportFailure::NoConfig { .. })
    ));

    let broken = tmp.path().join("broken");
    std::fs::create_dir_all(&broken).unwrap();
    std::fs::write(broken.join("config.txt"), "{\"ui\": {\"read-komi\": false,\n").unwrap();
    match java_import::read(&JavaSource::new(&broken.join("config.txt"), None)) {
        Err(ImportFailure::Invalid { detail, .. }) => assert!(detail.contains("line 2"), "{detail}"),
        other => panic!("{other:?}"),
    }

    // A broken `persist` only loses its own settings; a BOM is accepted like Java.
    std::fs::write(broken.join("config.txt"), "\u{feff}{\"ui\": {\"read-komi\": false}}").unwrap();
    std::fs::write(broken.join("persist"), "not json").unwrap();
    let c = java_import::read(&JavaSource::new(&broken, None)).unwrap();
    assert_eq!(c.mapped.read_komi, Some(false));
    let persist = c.items.iter().find(|i| i.key == "persist:").unwrap();
    assert_eq!(
        (persist.status, persist.reason),
        (ImportStatus::Incompatible, Some(ImportReason::Unreadable))
    );
}

#[test]
fn engines_from_an_old_java_profile_are_read_like_java() {
    // Before `engine-settings-list`, Java kept parallel lists (`Utils.getEngineDataOld`).
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("old");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "default-engine": 1, "autoload-default": true },
        "leelaz": {
            "engine-command": "first gtp",
            "engine-command-list": ["second gtp", ""],
            "engine-name-list": ["A", "B"],
            "engine-komi-list": [7.5, 0.5],
        },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let c = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let engines: Vec<_> = c.mapped.engines.unwrap().into_iter().map(|m| m.profile).collect();
    assert_eq!(engines.iter().map(|e| e.name.as_str()).collect::<Vec<_>>(), ["A", "B"]);
    assert_eq!(
        (engines[1].komi, engines[1].is_default, engines[0].is_default),
        (0.5, true, false)
    );
    assert_eq!(c.mapped.startup, Some(EngineStartup::Default));
    assert!(
        c.items.iter().all(|i| i.status != ImportStatus::NotMigrated),
        "{:?}",
        c.items
    );
}

#[test]
fn the_import_is_offered_on_first_use_until_declined() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.set_java_import_locations(vec![tmp.path().join("missing"), java.clone(), java.clone()]);
    assert_eq!(
        wb.view(false).java_import,
        JavaImportView {
            offer: true,
            candidates: vec![java.to_string_lossy().into_owned()],
        }
    );
    assert!(wb.dismiss_java_import_offer().unwrap());
    assert!(!wb.view(false).java_import.offer);
    assert!(!workbench(&product, Recorder::default()).view(false).java_import.offer);
}
