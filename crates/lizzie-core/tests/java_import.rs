//! Importing the fixed Java program's settings through the public workbench operations: the
//! Java files (`config.txt`, `persist`, written the way `Config.java` writes them) are only read;
//! this product's settings change only on a confirmed, complete import; credentials are copied
//! only when confirmed and never appear in the preview or report.

// The Java `config.txt` fixture is one large `json!` literal.
#![recursion_limit = "256"]

use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use lizzie_core::api::{EngineProblem, ImportFailure, ImportReason, ImportStatus, ImportTarget, JavaImportView};
use lizzie_core::engine::gtp::{split_command, LaunchSpec};
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineIo, InstanceId, Launcher};
use lizzie_core::java_import::{self, JavaSource};
use lizzie_core::settings::{
    Config, DisplaySettings, EstimateStyle, ExtraMode, HawkeyeBranch, HawkeyePeriod, Language, MoveRankMode, Persisted,
    ProblemSide, ProblemSort, ProxyMode, Storage, SubBoardMode,
};
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
            "append-winrate-to-comment": false,
            "play-sound": false,
            "allow-double-click": false,
            "allow-drag": true,
            "show-right-menu": false,
            "show-new-board-hint": false,
            "show-markup-tools": true,
            "other-size-width": 7,
            "other-size-height": 99,
            "no-capture": true,
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
            "enable-startup-benchmark": true,
            "network-proxy-mode": "manual",
            "network-proxy-host": "127.0.0.1",
            "network-proxy-port": "7897",
            "win-rate-always-black": true,
            "show-katago-score-lead-with-komi": true,
            "auto-quick-analyze-on-load": false,
            "analysis-engine-command": "engines/fake-katago analysis -model weights/model.bin.gz -config cfg/gtp.cfg",
            "analysis-engine-command-customized": true,
            "analysis-max-visits": 0,
            "analysis-reuse-current-engine": true,
            "quick-analysis-lightweight-model-enabled": true,
            "is-showing-blunder-table": true,
            "problem-list-side-filter": "white",
            "problem-list-sort": "move-desc",
            "problem-list-winrate-threshold": 5.5,
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
                    "threadPolicy": {
                        "source": "BENCHMARK", "sourceRevision": 3, "katago-benchmark-threads": 12,
                        "katago-benchmark-backend": "CUDA", "environment": { "engine": "engines/fake-katago" },
                    },
                    "gtpConfigurationProtocol": "katago-gtp-config-v1",
                    "gtpConfigurationProfile": { "maxVisits": 800 },
                    "managedProfileType": "bundled-katago",
                    "managedProfileCommand": "engines/fake-katago gtp",
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
    let images = item(&p.items, "persist:filesystem.last-image-folder");
    assert_eq!(
        (images.status, images.value.as_deref()),
        (ImportStatus::Imported, Some("/images"))
    );
    let window = item(&p.items, "persist:ui-persist.main-window-position");
    assert_eq!(
        (window.status, window.reason),
        (ImportStatus::Incompatible, Some(ImportReason::WindowCoordinates))
    );
    // An empty save folder holds no 存档与读档 slots.
    assert!(p.recovery.is_empty());

    // Engines: Java-relative paths resolved against the Java folder; problems found without
    // starting anything; the remote engine left out; the one with a token needs confirmation.
    let local = &p.engines[0];
    let tokens = split_command(&local.command);
    assert_eq!(tokens[0], java.join("engines/fake-katago").to_string_lossy());
    assert_eq!(tokens[3], java.join("weights/model.bin.gz").to_string_lossy());
    assert!(local.problems.is_empty() && local.is_default && local.unmigrated_fields.is_empty());
    assert_eq!(
        local.incompatible_fields,
        ["managedProfileCommand", "managedProfileType"]
    );
    for key in [
        "ui.enable-startup-benchmark",
        "ui.network-proxy-mode",
        "ui.win-rate-always-black",
        "ui.show-katago-score-lead-with-komi",
        "ui.network-proxy-port",
    ] {
        assert_eq!(status(key), ImportStatus::Imported, "{key}");
    }
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

    let report = wb.apply_java_import(p.id, &[], false, None).unwrap().unwrap();
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
    assert!(!c.record.append_winrate_to_comment);
    // 自动快析, 精析设置 (a relative command resolved in the Java folder), 轻量模型, 问题手.
    assert!(!c.record.auto_quick_analyze_on_load);
    let ae = &c.analysis_engine;
    assert!(
        ae.command.contains("Lizzie Next/engines/fake-katago") && ae.command.contains(" analysis "),
        "{}",
        ae.command
    );
    assert!(ae.command_customized && ae.reuse_current_engine && ae.max_visits == 1);
    assert!(c.quick_model.enabled);
    let pl = &c.problems;
    assert!(pl.showing && pl.side == ProblemSide::White && pl.sort == ProblemSort::MoveDesc);
    assert_eq!(pl.winrate_threshold, 5.5);
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
        c.display,
        DisplaySettings {
            winrate_always_black: true,
            score_lead_with_komi: true,
            ..DisplaySettings::default()
        }
    );
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
    // Thread policy with its benchmark results and the GTP configuration profile come along; the
    // Java package-management marker does not (this client did not install those files).
    let policy = local.thread_policy.as_ref().expect("thread policy");
    assert_eq!(
        (policy.source_revision, policy.threads, policy.backend.as_str()),
        (3, 12, "CUDA")
    );
    assert!(policy.environment.is_some());
    let gtp = local.gtp_configuration.as_ref().expect("gtp configuration");
    assert_eq!(
        (gtp.protocol.as_str(), gtp.profile.get("maxVisits")),
        ("katago-gtp-config-v1", Some(&json!(800)))
    );
    assert!(local.managed.is_none());
    assert!(c.engine.startup_benchmark);
    assert_eq!(
        (
            c.network.proxy_mode,
            c.network.proxy_host.as_str(),
            c.network.proxy_port
        ),
        (ProxyMode::Manual, "127.0.0.1", 7897)
    );
    assert_eq!((c.engine.engines[1].width, c.engine.engines[1].height), (9, 19));
    let e = &c.edit;
    assert_eq!(
        (
            e.allow_double_click,
            e.allow_drag,
            e.show_right_menu,
            e.show_new_board_hint,
            e.show_markup_tools,
            e.no_capture
        ),
        (false, true, false, false, true, true)
    );
    assert_eq!(
        (e.other_size_width, e.other_size_height),
        (7, 21),
        "a size Java's dialog cannot hold is not taken over"
    );
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
    let report = wb.apply_java_import(p.id, &[3], false, None).unwrap().unwrap();
    assert_eq!(report.engines[3].status, ImportStatus::Imported);
    let c = saved_config(&product);
    let proxy = c.engine.engines.iter().find(|e| e.name == "Proxy").unwrap();
    assert!(
        proxy.command.ends_with(SECRETS[2]),
        "the confirmed command is kept as written"
    );
    // A preview is applied once.
    assert_eq!(wb.apply_java_import(p.id, &[], false, None).unwrap(), Err(IMPORT_STALE));
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
        wb.apply_java_import(p.id, &[], false, None).unwrap(),
        Err(IMPORT_NEEDS_REPLACE)
    );
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), before);
    assert_eq!(wb.config().engine.engines[0].name, "Mine");

    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
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
    assert!(wb.apply_java_import(p.id, &[], true, None).is_err());
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
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
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
    assert!(wb.apply_java_import(p.id, &[], true, None).is_err());
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
        wb.apply_java_import(p.id, &[], false, None).unwrap(),
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
fn after_resetting_the_interface_position_only_the_imported_folder_is_saved() {
    // `Config.deletePersist`: the reset placement stays gone until the next start; the folder
    // the user imports is kept, even when it equals the folder used before the reset.
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let st = storage(&product);
    std::fs::create_dir_all(st.persist_file.parent().unwrap()).unwrap();
    let saved = json!({ "lastFolder": "/games", "window": { "x": 5, "y": 6, "width": 900, "height": 700, "maximized": false } });
    std::fs::write(&st.persist_file, saved.to_string()).unwrap();
    let mut wb = workbench(&product, Recorder::default());
    wb.reset_ui_position().unwrap();
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.persisted().last_folder.as_deref(), Some("/games"));
    assert_eq!(
        reopened.persisted().window,
        None,
        "the reset placement is not written back"
    );
}

#[test]
fn a_different_last_engine_choice_needs_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let java = java_dir(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[3], false, None).unwrap().unwrap();
    // The same Java settings, except the engine used last.
    let config = std::fs::read_to_string(java.join("config.txt")).unwrap();
    std::fs::write(
        java.join("config.txt"),
        config.replace("\"last-engine\": 0", "\"last-engine\": 1"),
    )
    .unwrap();
    let p = preview(&mut wb, &java);
    assert_eq!(p.replaces, [ImportTarget::LastEngine]);
    assert_eq!(
        wb.apply_java_import(p.id, &[], false, None).unwrap(),
        Err(IMPORT_NEEDS_REPLACE)
    );
    assert_eq!(wb.config().engine.last_engine.as_deref(), Some("java-0"));
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
    let report = wb.apply_java_import(p.id, &[], false, None).unwrap().unwrap();
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

#[test]
fn proxy_settings_java_would_reject_are_reported_and_not_taken_over() {
    let tmp = tempfile::tempdir().unwrap();
    let product = tmp.path().join("product");
    for (ui, valid) in [
        // `NetworkProxy.manualSettings`: manual needs a host and a port 1–65535.
        (
            json!({ "network-proxy-mode": "manual", "network-proxy-host": " ", "network-proxy-port": 8080 }),
            false,
        ),
        (
            json!({ "network-proxy-mode": "manual", "network-proxy-host": "proxy", "network-proxy-port": 70000 }),
            false,
        ),
        (json!({ "network-proxy-mode": "socks" }), false),
        // A missing mode is `direct`; system mode ignores the address fields.
        (json!({ "network-proxy-host": "proxy" }), true),
        (
            json!({ "network-proxy-mode": "system", "network-proxy-port": "8080" }),
            true,
        ),
    ] {
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({ "ui": ui.clone() }).to_string()).unwrap();
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        let proxy: Vec<_> = p
            .items
            .iter()
            .filter(|i| i.key.starts_with("ui.network-proxy-"))
            .collect();
        let expected = if valid {
            ImportStatus::Imported
        } else {
            ImportStatus::Incompatible
        };
        assert!(
            proxy
                .iter()
                .all(|i| i.status == expected && i.target == Some(ImportTarget::NetworkProxy)),
            "{ui}"
        );
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let network = saved_config(&product).network;
        if valid {
            let mode = if ui.get("network-proxy-mode").is_some() {
                ProxyMode::System
            } else {
                ProxyMode::Direct
            };
            assert_eq!(network.proxy_mode, mode, "{ui}");
        } else {
            assert_eq!(
                network,
                Default::default(),
                "{ui}: rejected values leave the setting as it was"
            );
        }
    }
}

#[test]
fn board_display_settings_are_read_like_java_including_older_keys() {
    let tmp = tempfile::tempdir().unwrap();
    let product = tmp.path().join("product");
    let defaults = DisplaySettings::default();
    for (ui, expect) in [
        // `allow-move-number` wins over the older pair (`Config.java:1601-1605`).
        (
            json!({ "show-move-number": true, "only-last-move-number": 5, "allow-move-number": 3 }),
            DisplaySettings { move_numbers: 3, ..defaults.clone() },
        ),
        // Without it: shown with a positive count is the last n, otherwise all; hidden is none.
        (
            json!({ "show-move-number": true, "only-last-move-number": 5 }),
            DisplaySettings { move_numbers: 5, ..defaults.clone() },
        ),
        (
            json!({ "show-move-number": true, "only-last-move-number": 0 }),
            DisplaySettings { move_numbers: -1, ..defaults.clone() },
        ),
        (
            json!({ "show-move-number": false, "only-last-move-number": 5 }),
            DisplaySettings { move_numbers: 0, ..defaults.clone() },
        ),
        // `MoveRankEvaluationMode.fromConfig`: a named mode, else the older two flags.
        (
            json!({ "move-rank-evaluation-mode": "combined", "use-score-loss-in-move-rank": false }),
            DisplaySettings { move_rank_mode: MoveRankMode::Combined, ..defaults.clone() },
        ),
        (
            json!({ "use-score-loss-in-move-rank": false }),
            DisplaySettings { move_rank_mode: MoveRankMode::Winrate, ..defaults.clone() },
        ),
        // 小棋盘: the heat map wins over stones only; after-calculation heat is the visits one.
        (
            json!({ "show-heat": true, "show-heat-aftercalc": true, "subboard-raw": true }),
            DisplaySettings { sub_board: SubBoardMode::Visits, ..defaults.clone() },
        ),
        (
            json!({ "show-heat": true }),
            DisplaySettings { sub_board: SubBoardMode::Policy, ..defaults.clone() },
        ),
        (
            json!({ "subboard-raw": true }),
            DisplaySettings { sub_board: SubBoardMode::Raw, ..defaults.clone() },
        ),
        // 纯网络形势: the painter Java's drawing would use when several flags are on.
        (
            json!({ "show-pure-estimate-by-size": true, "show-pure-estimate-big-below": true }),
            DisplaySettings { pure_estimate: EstimateStyle::BySize, ..defaults.clone() },
        ),
        (
            json!({
                "show-pure-estimate-big-below": false,
                "show-pure-estimate-normal": true,
                "show-pure-estimate-not-on-live": true,
            }),
            DisplaySettings { pure_estimate: EstimateStyle::TransparentNotOnLive, ..defaults.clone() },
        ),
        (
            json!({ "show-pure-estimate-big-below": false, "show-pure-estimate-small": true }),
            DisplaySettings { pure_estimate: EstimateStyle::Small, ..defaults.clone() },
        ),
        (
            json!({
                "show-coordinates": false,
                "move-rank-mark-last-move": 0,
                "txt-move-rank-mark-last-move": 4,
                "suggestion-info-winrate": 3,
                "suggestion-info-playouts": 1,
                "suggestion-info-scorelead": 2,
                "replay-branch-interval-seconds": 0.4,
                "min-playouts-for-next-move": 50,
            }),
            DisplaySettings {
                coordinates: false,
                move_rank: 0,
                move_rank_custom: 4,
                suggestion_info_order: [3, 1, 2],
                replay_interval_seconds: 0.4,
                next_move_min_playouts: 50,
                ..defaults.clone()
            },
        ),
    ] {
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({ "ui": ui.clone() }).to_string()).unwrap();
        std::fs::remove_dir_all(&product).ok();
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        for key in ui.as_object().unwrap().keys() {
            assert_eq!(item(&p.items, &format!("ui.{key}")).status, ImportStatus::Imported, "{ui}");
        }
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(saved_config(&product).display, expect, "{ui}");
    }
}

#[test]
fn board_display_values_java_would_not_use_are_reported_and_left_alone() {
    let tmp = tempfile::tempdir().unwrap();
    let product = tmp.path().join("product");
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let ui = json!({
        "move-rank-mark-last-move": -2,
        "txt-move-rank-mark-last-move": 0,
        "suggestion-info-winrate": 4,
        "show-coordinates": "maybe",
        "show-heat": "yes",
        "allow-move-number": "x",
        "show-pure-estimate-normal": "x",
    });
    std::fs::write(java.join("config.txt"), json!({ "ui": ui.clone() }).to_string()).unwrap();
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ui.as_object().unwrap().keys() {
        assert_eq!(item(&p.items, &format!("ui.{key}")).status, ImportStatus::Incompatible, "{key}");
    }
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    assert_eq!(saved_config(&product).display, DisplaySettings::default());
}

#[test]
fn remaining_java_display_keys_import_individually_and_persist() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    for (i, (section, key, value, field, expected)) in [
        ("ui", "use-i-in-coords-name", json!(true), "coordinateStyle", json!("WithI")),
        ("ui", "use-fox-style-coords", json!(true), "coordinateStyle", json!("Fox")),
        ("ui", "use-num-coords-from-top", json!(true), "coordinateStyle", json!("NumbersFromTop")),
        ("ui", "use-num-coords-from-bottom", json!(true), "coordinateStyle", json!("NumbersFromBottom")),
        ("ui", "show-blue-ring", json!(false), "blueRing", json!(false)),
        ("ui", "suggestion-color-ratio", json!(3), "suggestionColorRatio", json!(3)),
        ("ui", "white-suggestion-order-white", json!(true), "whiteSuggestionOrderWhite", json!(true)),
        ("leelaz", "show-nosugg-circle", json!(true), "noSuggCircle", json!(true)),
        ("ui", "show-pv-visits", json!(true), "pvVisits", json!("Off")),
        ("ui", "show-pv-visits-last-move", json!(true), "pvVisits", json!("LastMove")),
        ("ui", "show-pv-visits-all-move", json!(true), "pvVisits", json!("AllMoves")),
        ("ui", "pv-visits-limit", json!(-7), "pvVisitsLimit", json!(-7)),
        ("ui", "remove-dead-in-variation", json!(false), "removeDeadInVariation", json!(false)),
        ("ui", "auto-replay-branch", json!(true), "autoReplayBranch", json!(true)),
        ("ui", "auto-replay-display-entire-variations-first", json!(true), "entireVariationFirst", json!(true)),
        ("ui", "display-entire-variations-first-seconds", json!(1.25), "entireVariationFirstSeconds", json!(1.25)),
        ("ui", "continue-with-best-move", json!(true), "continueWithBestMove", json!(true)),
        ("ui", "directly-with-best-move", json!(true), "directlyWithBestMove", json!(true)),
        ("ui", "min-playouts-for-next-move", json!(i32::MIN), "nextMoveMinPlayouts", json!(i32::MIN)),
        ("ui", "show-win-rate-line", json!(false), "winrateLine", json!(false)),
        ("ui", "show-score-lead-line", json!(false), "scoreLeadLine", json!(false)),
        ("ui", "show-winrate-graph-fill", json!(false), "winrateGraphFill", json!(false)),
        ("ui", "show-mouse-over-winrate-graph", json!(false), "mouseOverWinrateGraph", json!(false)),
        ("ui", "initial-max-score-lead", json!(30.5), "initialMaxScoreLead", json!(30.5)),
        ("ui", "use-moves-ownership", json!(false), "useMovesOwnership", json!(false)),
        ("ui", "shortcut-kata-estimate", json!(true), "kataEstimateShortcut", json!(true)),
        ("ui", "save-kata-estimate-status", json!(true), "kataEstimateSaveStatus", json!(true)),
        ("ui", "show-katago-estimate-big-below", json!(true), "kataEstimateStyle", json!("BigBelow")),
        ("ui", "use-territory-in-score", json!(true), "territoryScoring", json!(true)),
        ("ui", "match-ai-moves", json!(5), "matchAiMoves", json!(5)),
        ("ui", "match-ai-percents-playouts", json!(12.5), "matchAiPercentsPlayouts", json!(12.5)),
        ("ui", "show-suggestions-frame", json!(true), "suggestionsFrame", json!(true)),
        ("ui", "suggestions-always-ontop", json!(true), "suggestionsOnTop", json!(true)),
        ("ui", "show-bestmoves-list", json!(false), "bestMovesList", json!(false)),
        ("ui", "show-bestmoves-graph", json!(true), "bestMovesGraph", json!(true)),
        ("ui", "anaframe-show-next", json!(false), "frameShowNext", json!(false)),
        ("ui", "anaframe-use-mousemove", json!(true), "frameMouseMove", json!(true)),
    ].into_iter().enumerate() {
        std::fs::write(java.join("config.txt"), json!({section: {key: value}}).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{i}"));
        let mut wb = workbench(&product, Recorder::default());
        let preview = preview(&mut wb, &java);
        assert_eq!(item(&preview.items, &format!("{section}.{key}")).status, ImportStatus::Imported);
        wb.apply_java_import(preview.id, &[], true, None).unwrap().unwrap();
        let actual = serde_json::to_value(&saved_config(&product).display).unwrap();
        assert_eq!(actual[field], expected, "{key}");
        assert_eq!(workbench(&product, Recorder::default()).config().display, saved_config(&product).display);
    }
}

#[test]
fn java_display_flag_precedence_follows_the_board_consumers() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    for (i, (ui, field, expected)) in [
        (json!({"use-num-coords-from-top": true, "use-num-coords-from-bottom": true, "use-fox-style-coords": true, "use-i-in-coords-name": true}), "coordinateStyle", "NumbersFromTop"),
        (json!({"use-num-coords-from-bottom": true, "use-fox-style-coords": true, "use-i-in-coords-name": true}), "coordinateStyle", "NumbersFromTop"),
        (json!({"use-num-coords-from-bottom": true, "use-i-in-coords-name": true}), "coordinateStyle", "NumbersFromBottom"),
        (json!({"use-fox-style-coords": true, "use-i-in-coords-name": true}), "coordinateStyle", "Fox"),
        (json!({"show-pv-visits": false, "show-pv-visits-last-move": true, "show-pv-visits-all-move": true}), "pvVisits", "AllMoves"),
        (json!({"show-pv-visits": false, "show-pv-visits-last-move": true}), "pvVisits", "LastMove"),
        (json!({"show-pv-visits": true, "show-pv-visits-last-move": false, "show-pv-visits-all-move": false}), "pvVisits", "Off"),
    ].into_iter().enumerate() {
        std::fs::write(java.join("config.txt"), json!({"ui": ui}).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{i}"));
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        assert!(p.items.iter().all(|item| item.status == ImportStatus::Imported));
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(serde_json::to_value(wb.config().display.clone()).unwrap()[field], json!(expected));
    }
}

#[test]
fn java_estimate_and_blunder_bar_flags_follow_java_loading() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    for (i, (ui, field, expected)) in [
        // Kata形势 squares as `drawEstimate` chooses them; the normal flag is never consulted.
        (json!({"show-katago-estimate-small": true, "show-katago-estimate-by-size": true, "show-katago-estimate-big-below": true}), "kataEstimateStyle", json!("BySize")),
        (json!({"show-katago-estimate-small": true, "show-katago-estimate-big-below": true}), "kataEstimateStyle", json!("BigBelow")),
        (json!({"show-katago-estimate-small": true, "show-katago-estimate-not-on-live": true}), "kataEstimateStyle", json!("Small")),
        (json!({"show-katago-estimate-small": false, "show-katago-estimate-not-on-live": true}), "kataEstimateStyle", json!("TransparentNotOnLive")),
        (json!({"show-katago-estimate-small": false, "show-katago-estimate-normal": false}), "kataEstimateStyle", json!("Transparent")),
        // Java hides the blunder bar once for a profile not yet migrated.
        (json!({"show-blunder-bar": true}), "blunderBar", json!(false)),
        (json!({"show-blunder-bar": true, "migrated-hide-blunder-bar-default-v1": true}), "blunderBar", json!(true)),
        (json!({"show-katago-estimate": true, "show-katago-estimate-onmainboard": false, "show-katago-estimate-onsubbord": true}), "kataEstimate", json!({"shown": true, "onMain": false, "onSub": true})),
    ].into_iter().enumerate() {
        std::fs::write(java.join("config.txt"), json!({"ui": ui}).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{i}"));
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        assert!(p.items.iter().all(|item| item.status == ImportStatus::Imported), "{ui}");
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(serde_json::to_value(wb.config().display.clone()).unwrap()[field], expected, "{ui}");
    }
}

#[test]
fn imported_kata_estimate_applies_to_the_running_session() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    for (shown, main, sub, hidden) in [(true, true, false, false), (true, false, false, true), (false, true, true, false)] {
        let ui = json!({"show-katago-estimate": shown, "show-katago-estimate-onmainboard": main, "show-katago-estimate-onsubbord": sub});
        std::fs::write(java.join("config.txt"), json!({ "ui": ui }).to_string()).unwrap();
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let v = wb.view(false).kata_estimate;
        assert_eq!((v.state.shown, v.state.on_main, v.state.on_sub, v.hidden), (shown, main, sub, hidden), "{ui}");
    }
}

#[test]
fn the_java_layout_mode_imports_double_engine_mode_and_reports_layouts_not_here() {
    // `Config.readExtraMode`: numbers or names; anything unknown is 默认模式.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    // Unsupported layouts follow different modes: they leave whichever mode is current.
    for (value, status, mode) in [
        (json!(2), ImportStatus::Imported, ExtraMode::DoubleEngine),
        (json!(3), ImportStatus::NotMigrated, ExtraMode::DoubleEngine),
        (json!("0"), ImportStatus::Imported, ExtraMode::Normal),
        (json!("Four_Sub"), ImportStatus::NotMigrated, ExtraMode::Normal),
        (json!("Double_Engine"), ImportStatus::Imported, ExtraMode::DoubleEngine),
        (json!(99), ImportStatus::Imported, ExtraMode::Normal),
    ] {
        std::fs::write(
            java.join("config.txt"),
            json!({ "ui": { "extra-mode": value } }).to_string(),
        )
        .unwrap();
        let p = preview(&mut wb, &java);
        assert_eq!(item(&p.items, "ui.extra-mode").status, status, "{value}");
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(wb.view(false).settings.extra_mode, mode, "{value}");
        assert_eq!(saved_config(&product).ui.extra_mode, mode, "{value}");
    }
}

#[test]
fn new_java_display_keys_report_invalid_values_without_changing_defaults() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let ui = json!({
        "use-i-in-coords-name": "invalid", "use-fox-style-coords": "invalid",
        "use-num-coords-from-top": "invalid", "use-num-coords-from-bottom": "invalid",
        "show-blue-ring": "invalid", "suggestion-color-ratio": 4,
        "white-suggestion-order-white": "invalid", "show-pv-visits": "invalid",
        "show-pv-visits-last-move": "invalid", "show-pv-visits-all-move": "invalid",
        "pv-visits-limit": 2147483648_i64, "remove-dead-in-variation": "invalid",
        "auto-replay-branch": "invalid", "auto-replay-display-entire-variations-first": "invalid",
        "display-entire-variations-first-seconds": "invalid",
        "continue-with-best-move": "invalid", "directly-with-best-move": "invalid",
        "min-playouts-for-next-move": -2147483649_i64,
    });
    std::fs::write(java.join("config.txt"), json!({"ui": ui.clone(), "leelaz": {"show-nosugg-circle": "invalid"}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ui.as_object().unwrap().keys() {
        assert_eq!(item(&p.items, &format!("ui.{key}")).status, ImportStatus::Incompatible, "{key}");
    }
    assert_eq!(item(&p.items, "leelaz.show-nosugg-circle").status, ImportStatus::Incompatible);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    assert_eq!(saved_config(&product).display, DisplaySettings::default());
}

#[test]
fn autoplay_intervals_import_from_java_persist_toolbar_slots() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    for (i, (main, sub, valid)) in [(2, 350, true), (-1, -1, true), (0, -3, false)].into_iter().enumerate() {
        let mut toolbar = vec![json!(-1); 51];
        toolbar[40] = json!(main);
        toolbar[41] = json!(sub);
        std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{i}"));
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        for index in [40, 41] {
            assert_eq!(item(&p.items, &format!("persist:ui-persist.toolbar-parameter[{index}]")).status,
                if valid { ImportStatus::Imported } else { ImportStatus::Incompatible });
        }
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(wb.config().display.autoplay_main_seconds, if valid { f64::from(main) } else { -1.0 });
        assert_eq!(wb.config().display.autoplay_sub_millis, if valid { sub } else { -1 });
    }
}

#[test]
fn an_analysis_command_without_its_customized_flag_is_judged_like_older_java() {
    // `AnalysisEngineCommandHelper.isAnalysisCommandCustomized`: without the flag, a command
    // other than the default one counts as the user's.
    const DEFAULT: &str = "katago analysis -model model.bin.gz -config analysis.cfg -quit-without-waiting";
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let own = "katago analysis -model mine.bin.gz -config a.cfg";
    for (i, (command, flag, customized)) in [
        (own, None, true),
        (DEFAULT, None, false),
        (
            "  katago  analysis -model model.bin.gz -config analysis.cfg -quit-without-waiting ",
            None,
            false,
        ),
        (own, Some(false), false),
        (DEFAULT, Some(true), true),
    ]
    .into_iter()
    .enumerate()
    {
        let mut ui = json!({ "analysis-engine-command": command });
        if let Some(flag) = flag {
            ui["analysis-engine-command-customized"] = json!(flag);
        }
        std::fs::write(java.join("config.txt"), json!({ "ui": ui }).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{i}"));
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(
            wb.config().analysis_engine.command_customized,
            customized,
            "{command:?} {flag:?}"
        );
    }
}

#[test]
fn a_relative_lightweight_model_path_is_found_in_the_java_folder() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    let model = java.join("quick-analysis-models/b10c384h6nbttflrs.bin.gz");
    std::fs::create_dir_all(model.parent().unwrap()).unwrap();
    std::fs::write(&model, b"model").unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": { "katago-quick-analysis-model-path": "quick-analysis-models/b10c384h6nbttflrs.bin.gz" } })
            .to_string(),
    )
    .unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    assert_eq!(
        wb.config().quick_model.path.as_deref().map(Path::new),
        Some(model.as_path())
    );
    assert_eq!(
        saved_config(&product).quick_model.path,
        wb.config().quick_model.path,
        "kept after a restart"
    );
    assert_eq!(std::fs::read(&model).unwrap(), b"model", "the Java file is untouched");
}

#[test]
fn the_move_range_of_flash_analysis_imports_like_java_reads_it() {
    // `Config.java:1855-1856`: `optInt`, so a numeric string counts and a word is unusable.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": { "analysis-start-move": "30", "analysis-end-move": "end" } }).to_string(),
    )
    .unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.analysis-start-move").status, ImportStatus::Imported);
    assert_eq!(item(&p.items, "ui.analysis-end-move").status, ImportStatus::Incompatible);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let range = wb.config().flash_range;
    assert_eq!((range.start_move, range.end_move), (30, -1));
    assert_eq!(saved_config(&product).flash_range, range, "kept after a restart");
}

#[test]
fn automatic_analysis_imports_conditions_and_toolbar_parameters_transactionally() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({ "ui": {
        "analyze-all-branch": true,
        "auto-ana-diff-enable": true, "auto-ana-diff-black": false,
        "auto-ana-diff-white": true, "auto-ana-diff-use-win": false,
        "auto-ana-diff-use-score": true, "auto-ana-diff-win-threshold": "12.5",
        "auto-ana-diff-score-threshold": 4.5, "auto-ana-diff-time": 20,
        "auto-ana-diff-playouts": 5000, "auto-ana-diff-first-playouts": 2000,
        "exit-auto-analyze-by-pause": false, "exit-auto-analyze-tip": false
    }}).to_string()).unwrap();
    let mut toolbar = vec![json!(-1); 51];
    for (index, value) in [(0, 15), (1, 80), (2, 1), (3, 4), (4, 1), (5, 1), (6, 300), (7, 1), (8, 200)] {
        toolbar[index] = json!(value);
    }
    toolbar[47] = json!(false); toolbar[48] = json!(true);
    std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let before = wb.config().auto_analysis.clone();
    let p = preview(&mut wb, &java);
    assert_eq!(wb.config().auto_analysis, before, "preview writes nothing");
    for key in ["ui.analyze-all-branch", "ui.auto-ana-diff-time", "ui.auto-ana-diff-win-threshold", "ui.exit-auto-analyze-tip", "persist:ui-persist.toolbar-parameter[0]", "persist:ui-persist.toolbar-parameter[48]"] {
        let row = item(&p.items, key);
        assert_eq!(row.status, ImportStatus::Imported, "{key}");
        assert_eq!(row.target, Some(ImportTarget::AutoAnalysis));
    }
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let s = &wb.config().auto_analysis;
    assert_eq!((s.start_move, s.end_move, s.time_seconds, s.visits, s.first_visits), (15, 80, 4, 300, 200));
    assert!(s.time_enabled && s.visits_enabled && s.first_visits_enabled && s.auto_save);
    assert!(!s.black && s.white && s.all_branches && s.diff_enable);
    assert!(!s.diff_black && s.diff_white && !s.diff_use_win && s.diff_use_score);
    assert_eq!((s.diff_win_threshold, s.diff_score_threshold, s.diff_time, s.diff_visits, s.diff_first_visits), (12.5, 4.5, 20, 5000, 2000));
    assert!(!s.exit_by_pause && !s.exit_tip);
    assert_eq!(&saved_config(&product).auto_analysis, s, "persisted on import");
}

#[test]
fn whole_game_visits_imports_with_java_from_stored_validation() {
    for (stored, expected) in [(3000, 3000), (499, 1000), (-1, 1000), (1_000_001, 1000)] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({"ui":{"whole-game-analysis-deep-visits":stored}}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        let row = item(&p.items, "ui.whole-game-analysis-deep-visits");
        assert_eq!(row.status, ImportStatus::Imported);
        assert_eq!(row.target, Some(lizzie_core::api::ImportTarget::WholeGameVisits));
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(wb.config().whole_game.deep_visits, expected);
        assert_eq!(saved_config(&product).whole_game.deep_visits, expected);
    }
}

#[test]
fn hawkeye_settings_import_like_java_reads_them() {
    // Config.java:1801-1827, 1891-1893, 1947-1956, 2021-2024; MoveListFrame.setFilterStatus.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": {
            "show-badmoves-frame": true,
            "badmoves-always-ontop": true,
            "badmoves-always-on-top": false,
            "movelist-selected-indextop": 2,
            "movelist-selected-index": 7,
            "moveList-selected-branch": 1,
            "show-movelist-matchai": false,
            "show-winrate-matchai": true,
            "move-list-winrate-threshold": 5,
            "move-list-score-threshold": 2,
            "move-list-visits-threshold": 100,
            "movelist-top-curnode": true,
            "opening-end-move": 50,
            "middle-end-move": 150,
            "game-statistics-custom-start": 10,
            "game-statistics-custom-end": 200,
            "winrate-diff-range1": 4,
            "winrate-diff-range2": 12,
            "score-diff-range1": 3,
            "score-diff-range2": 6,
            "loss-panel-select-winrate": true,
            "move-list-filter-current": false,
            "match-ai-firstmove": 50,
            "match-ai-lastmove": 150
        } })
        .to_string(),
    )
    .unwrap();
    std::fs::write(
        java.join("persist"),
        json!({ "ui-persist": { "badmoves-list-position": [10, 20, 900, 600, 3, 40, 40, 40, 40, 40, 40, 40, 40, 40] } })
            .to_string(),
    )
    .unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ["show-badmoves-frame", "badmoves-always-ontop", "moveList-selected-branch", "match-ai-firstmove"] {
        let it = item(&p.items, &format!("ui.{key}"));
        assert_eq!((it.status, it.target), (ImportStatus::Imported, Some(ImportTarget::Hawkeye)), "{key}");
    }
    // Java writes this key but never reads it.
    let unused = item(&p.items, "ui.badmoves-always-on-top");
    assert_eq!((unused.status, unused.reason), (ImportStatus::NotMigrated, Some(ImportReason::Obsolete)));
    let window = item(&p.items, "persist:ui-persist.badmoves-list-position");
    assert_eq!(
        (window.status, window.reason),
        (ImportStatus::Incompatible, Some(ImportReason::WindowCoordinates))
    );
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let h = wb.config().hawkeye.clone();
    assert!(h.show_frame && h.always_on_top && !h.show_list && h.show_graph && h.top_current_move && h.loss_winrate);
    assert_eq!((h.top_page, h.chart, h.branch), (2, 7, HawkeyeBranch::Current));
    assert_eq!((h.winrate_threshold, h.score_threshold, h.visits_threshold), (5, 2, 100));
    assert_eq!((h.opening_end_move, h.middle_end_move), (50, 150));
    assert_eq!((h.winrate_range1, h.winrate_range2, h.score_range1, h.score_range2), (4, 12, 3, 6));
    // 50..150 is the saved 中盘 range for these period ends.
    assert_eq!(h.period, HawkeyePeriod::Middle);
    assert_eq!(saved_config(&product).hawkeye, h, "kept after a restart");
}

#[test]
fn an_unmatched_hawkeye_range_becomes_a_custom_period() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": { "match-ai-firstmove": 29, "match-ai-lastmove": 90, "movelist-selected-index": 9 } }).to_string(),
    )
    .unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.movelist-selected-index").status, ImportStatus::Incompatible);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let h = &wb.config().hawkeye;
    assert_eq!((h.period, h.custom_start, h.custom_end, h.chart), (HawkeyePeriod::Custom, 30, 90, 0));
}
