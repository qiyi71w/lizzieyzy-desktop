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
            "batch-analysis-playouts": 300,
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
    assert_eq!(ae.batch_visits, 300, "批量精析 每手总计算量");
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
fn the_java_layout_modes_import_numbers_and_names_and_unknown_values_use_normal() {
    // `Config.readExtraMode`: numbers or names; anything unknown is 默认模式.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    for (value, status, mode) in [
        (json!(2), ImportStatus::Imported, ExtraMode::DoubleEngine),
        (json!(3), ImportStatus::Imported, ExtraMode::Thinking),
        (json!("0"), ImportStatus::Imported, ExtraMode::Normal),
        (json!("Four_Sub"), ImportStatus::Imported, ExtraMode::FourSub),
        (json!("Double_Engine"), ImportStatus::Imported, ExtraMode::DoubleEngine),
        (json!(7), ImportStatus::Imported, ExtraMode::Min),
        (json!(8), ImportStatus::Imported, ExtraMode::FloatBoard),
        (json!("Thinking"), ImportStatus::Imported, ExtraMode::Thinking),
        (json!("Min"), ImportStatus::Imported, ExtraMode::Min),
        (json!("Float_Board"), ImportStatus::Imported, ExtraMode::FloatBoard),
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
fn region_depths_and_tracking_import_with_the_older_tracking_key_and_their_own_avoid_key() {
    // `Config.java:1879-1882` (Java reads the avoid custom depth from the allow key; its own key
    // is imported here), `migrateTrackingAnalysisConfig` and `loadTrackingPointAppearanceConfig`.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": {
            "select-allow-moves": 1, "select-avoid-moves": 7,
            "select-allow-custom-moves": 3, "select-avoid-custom-moves": 9,
            "tracking-engine-max-visits": 800,
            "show-tracking-point-outline": false, "tracking-point-outline-opacity": 150,
        }})
        .to_string(),
    )
    .unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.select-avoid-custom-moves").status, ImportStatus::Imported);
    assert_eq!(item(&p.items, "ui.tracking-engine-max-visits").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let region = wb.config().region;
    assert_eq!(
        (region.allow_moves, region.avoid_moves, region.allow_custom_moves, region.avoid_custom_moves),
        (1, 7, 3, 9)
    );
    let tracking = wb.config().tracking;
    assert_eq!((tracking.max_visits, tracking.show_outline, tracking.outline_opacity), (800, false, 100));
    assert_eq!(saved_config(&product).tracking, tracking, "kept after a restart");

    // The current key wins over the older one; 0 is the default.
    std::fs::write(
        java.join("config.txt"),
        json!({ "ui": { "tracking-engine-max-visits": 800, "tracking-analysis-max-visits": 0 } }).to_string(),
    )
    .unwrap();
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    assert_eq!(wb.config().tracking.max_visits, 500);
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
fn toolbar_sentinels_keep_javas_control_defaults() {
    // BottomToolbar.java:2385-2412: start replaces "1" only when >= 0; end, time and visits
    // replace their fields only when > 0; the condition boxes are only ever switched on.
    struct Case { slots: [i64; 9], start: i32, end: i32, time: (bool, i32), visits: (bool, i32), first: (bool, i32), save: bool }
    let cases = [
        Case { slots: [-1, -1, 1, -1, -1, -1, -1, -1, -1], start: 1, end: -1, time: (true, 2), visits: (false, -1), first: (false, -1), save: false },
        Case { slots: [0, 0, 0, 0, 0, 0, 0, 0, 0], start: 0, end: -1, time: (true, 2), visits: (false, -1), first: (false, -1), save: false },
        Case { slots: [7, 90, -1, 5, 1, 1, 400, -1, 50], start: 7, end: 90, time: (true, 5), visits: (true, 400), first: (false, 50), save: true },
    ];
    for (i, c) in cases.iter().enumerate() {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({ "ui": {} }).to_string()).unwrap();
        let mut toolbar = vec![json!(-1); 51];
        for (index, value) in c.slots.iter().enumerate() { toolbar[index] = json!(value); }
        toolbar[47] = json!(true); toolbar[48] = json!(true);
        std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let s = wb.config().auto_analysis.clone();
        assert_eq!((s.start_move, s.end_move), (c.start, c.end), "case {i}");
        assert_eq!(((s.time_enabled, s.time_seconds), (s.visits_enabled, s.visits), (s.first_visits_enabled, s.first_visits)), (c.time, c.visits, c.first), "case {i}");
        assert_eq!(s.auto_save, c.save, "case {i}");
        // The restored conditions are enough to start; only the missing engine refuses it.
        assert_eq!(wb.start_auto_analysis(s).unwrap(), Err("AutoAnalyze.engineUnavailable"), "case {i}");
    }
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
fn an_unmatched_hawkeye_range_selects_custom_with_javas_custom_moves() {
    // setFilterStatus picks 其他 for 29..90, then applies game-statistics-custom-start/end.
    for (custom, expected) in [(json!({}), (10, 200)), (json!({ "game-statistics-custom-start": 40, "game-statistics-custom-end": 120 }), (40, 120))] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        let mut ui = json!({ "match-ai-firstmove": 29, "match-ai-lastmove": 90, "movelist-selected-index": 9 });
        ui.as_object_mut().unwrap().extend(custom.as_object().unwrap().clone());
        std::fs::write(java.join("config.txt"), json!({ "ui": ui }).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        assert_eq!(item(&p.items, "ui.movelist-selected-index").status, ImportStatus::Incompatible);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let h = &wb.config().hawkeye;
        assert_eq!((h.period, (h.custom_start, h.custom_end), h.chart), (HawkeyePeriod::Custom, expected, 0));
    }
}

#[test]
fn workbench_panels_and_persisted_shares_are_imported_and_consumed_after_restart() {
    use lizzie_core::settings::{BottomToolbar, PanelSizes};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {
        "show-subboard": false, "show-winrate-graph": false, "large-winrate-graph": true,
        "show-comment": false, "show-variation-graph": false, "show-list-pane": false,
        "show-captured": false, "show-status": false, "large-subboard": true,
        "show-top-tool-bar": false, "show-double-menu": false, "mains-always-ontop": true,
        "show-title-wr": false, "hide-subboard-from-large-winrate": true
    }}).to_string();
    let persisted = json!({"ui-persist": {
        "leftover-left-share": 0.25, "board-postion-propotion": 2,
        "comment-height-share": 0.6, "variation-graph-share": 0.75,
        "main-window-other": [70, 1, 2, 3, 4]
    }}).to_string();
    std::fs::write(java.join("config.txt"), &source).unwrap();
    std::fs::write(java.join("persist"), &persisted).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ["ui.show-subboard", "ui.show-title-wr", "ui.show-top-tool-bar",
        "persist:ui-persist.leftover-left-share", "persist:ui-persist.comment-height-share",
        "persist:ui-persist.variation-graph-share", "persist:ui-persist.board-postion-propotion",
        "persist:ui-persist.main-window-other[0]"] {
        assert_eq!(item(&p.items, key).status, ImportStatus::Imported, "{key}");
    }
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    drop(wb);
    let s = workbench(&product, Recorder::default()).view(false).settings;
    assert!(!s.panels.show_sub_board && !s.panels.show_winrate_graph && !s.panels.show_comment);
    assert!(!s.panels.show_variation_graph && !s.panels.show_list_pane && !s.panels.show_captured);
    assert!(!s.panels.show_status && !s.panels.show_top_toolbar && !s.panels.show_double_menu);
    assert!(s.panels.large_winrate_graph && s.panels.large_sub_board && s.panels.always_on_top);
    assert!(!s.panels.show_winrate_in_title && s.panels.hide_sub_board_from_large_winrate);
    assert_eq!(s.panel_sizes, PanelSizes { leftover_left_share: Some(0.25), board_position_proportion: 2,
        comment_height_share: Some(0.6), variation_graph_share: Some(0.75) });
    assert_eq!(s.bottom_toolbar, BottomToolbar::Detailed);
    assert_eq!(std::fs::read_to_string(java.join("config.txt")).unwrap(), source);
    assert_eq!(std::fs::read_to_string(java.join("persist")).unwrap(), persisted);
}

#[test]
fn imported_double_engine_comment_policy_and_invalid_layout_values_do_not_leak() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {
        "extra-mode": 2, "show-comment": true, "show-status": "not-a-bool"
    }}).to_string()).unwrap();
    std::fs::write(java.join("persist"), json!({"ui-persist": {
        "leftover-left-share": 1.5, "board-postion-propotion": -5,
        "comment-height-share": "invalid", "variation-graph-share": -0.2
    }}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.show-status").status, ImportStatus::Incompatible);
    assert_eq!(item(&p.items, "persist:ui-persist.comment-height-share").status, ImportStatus::Incompatible);
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let s = wb.view(false).settings;
    assert!(s.comment_panel_auto_hidden && !s.panels.show_comment && s.panels.show_status);
    assert_eq!(s.panel_sizes.leftover_left_share, Some(1.0));
    assert_eq!(s.panel_sizes.board_position_proportion, 0);
    assert_eq!(s.panel_sizes.comment_height_share, None);
    assert_eq!(s.panel_sizes.variation_graph_share, Some(0.0));
}

#[test]
fn appearance_import_applies_defaults_styles_fonts_and_owned_images() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(java.join("theme")).unwrap();
    std::fs::write(java.join("theme/selected.png"), include_bytes!("../../../ui/public/assets/board.png")).unwrap();
    let input = json!({"ui": {
        "theme":"default", "board-style":"chinese-classic", "is-apple-style":true,
        "use-morandi-colors":true, "custom-board-image":"selected.png",
        "custom-window-background-image":"missing.jpg", "font-name":"Board Font",
        "ui-font-name":"UI Font", "winrate-font-name":"Win Font", "frame-font-size":16,
        "glass-effect-level":0, "glass-blur-radius":-1, "liquid-blur-radius":8,
        "glass-corner-radius":1, "glass-panel-overlay-color":[1,2,3,100],
        "comment-font-size":21, "winrate-stroke-width":2.5, "use-pure-board":true,
        "pure-board-color":[100,110,120], "blunder-winrate-thresholds":[-10,100],
        "blunder-node-colors":[[100,0,0],[0,100,0]]
    }}).to_string();
    std::fs::write(java.join("config.txt"), &input).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items,"ui.custom-board-image").status, ImportStatus::Imported);
    assert_eq!(item(&p.items,"ui.custom-window-background-image").status, ImportStatus::Incompatible);
    assert_eq!(item(&p.items,"file:theme").status, ImportStatus::NotMigrated);
    wb.apply_java_import(p.id,&[],true,None).unwrap().unwrap();
    let s = wb.view(true).settings;
    assert_eq!(s.appearance.board_style,lizzie_core::appearance::BoardStyle::ChineseClassic);
    assert!(s.appearance.apple_style && s.appearance.morandi_colors);
    assert_eq!(s.appearance.frame_font_size,16); assert_eq!(s.active_frame_font_size,12);
    assert_eq!((&*s.theme.font_name,&*s.theme.ui_font_name,&*s.theme.winrate_font_name),("Board Font","UI Font","Win Font"));
    assert_eq!((s.theme.comment_font_size,s.theme.winrate_stroke_width),(21,2.5));
    assert_eq!(s.theme.blunder_winrate_thresholds,vec![-10.,100.]);
    assert!(s.theme.use_pure_board); assert_eq!(s.theme.pure_board_color,vec![100,110,120]);
    assert_eq!(s.appearance.glass_effect_level,0);
    assert_eq!((s.theme.glass_blur_radius,s.theme.liquid_blur_radius,s.theme.glass_corner_radius),(0,8,4));
    assert_eq!(s.theme.glass_panel_overlay_color,vec![1,2,3,100]);
    assert_eq!(std::fs::read_to_string(java.join("config.txt")).unwrap(),input);
    std::fs::remove_file(java.join("theme/selected.png")).unwrap();
    let mut reopened = workbench(&product, Recorder::default());
    let v = reopened.view(true).settings;
    assert_eq!(v.active_frame_font_size,16);
    assert!(lizzie_core::appearance::images(reopened.storage(),&v.appearance,&v.theme).board.starts_with("data:image/png;"));
}

#[test]
fn importing_a_bundled_theme_selects_it_but_reports_java_theme_folders_separately() {
    let tmp=tempfile::tempdir().unwrap(); let java=tmp.path().join("java");
    std::fs::create_dir_all(java.join("theme/Personal")).unwrap();
    std::fs::write(java.join("config.txt"),json!({"ui":{"theme":"sabaki"}}).to_string()).unwrap();
    let mut wb=workbench(&tmp.path().join("product"),Recorder::default());
    let p=preview(&mut wb,&java);
    wb.apply_java_import(p.id,&[],true,None).unwrap().unwrap();
    let v=wb.view(true).settings;
    assert_eq!(v.appearance.theme,"sabaki"); assert_eq!(v.theme.shadow_size,100);
    assert_eq!(v.theme.stone_indicator_type,0);
}

#[test]
fn appearance_import_requires_replacement_and_keeps_failed_image_out_of_configuration() {
    let tmp=tempfile::tempdir().unwrap(); let java=tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"),json!({"ui":{
        "font-name":"Imported Font", "custom-board-image":"missing.png", "theme":"UnknownPersonalTheme"
    }}).to_string()).unwrap();
    let mut wb=workbench(&tmp.path().join("product"),Recorder::default());
    wb.set_frame_font_size(16).unwrap();
    let p=preview(&mut wb,&java);
    assert!(p.replaces.contains(&ImportTarget::Appearance));
    assert_eq!(item(&p.items,"ui.theme").status,ImportStatus::NotMigrated);
    assert_eq!(item(&p.items,"ui.custom-board-image").reason,Some(ImportReason::Unreadable));
    assert_eq!(wb.apply_java_import(p.id,&[],false,None).unwrap(),Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(wb.view(false).settings.theme.font_name,"Lizzie Default");
    wb.apply_java_import(p.id,&[],true,None).unwrap().unwrap();
    let s=wb.view(false).settings;
    assert_eq!(s.theme.font_name,"Imported Font"); assert!(s.appearance.custom_board_image.is_empty());
    assert_eq!(s.appearance.theme,"default");
}

#[test]
fn imported_custom_layouts_restore_java_snapshot_fields_and_independent_bounds() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let snapshot = json!({
        "show-subboard": false, "show-winrate-graph": true, "show-comment": true,
        "show-status": false, "show-variation-graph": true, "show-captured": false,
        "show-list-pane": true, "large-subboard": false, "large-winrate-graph": false,
        "board-position-proportion": 6, "main-frame-maxsize": false,
        "main-frame-position": [10, 20, 600, 800], "independent-main-board": true,
        "independent-sub-board": true, "independent-main-board-position": [30, 40, 700, 700],
        "independent-sub-board-position": [50, 60, 400, 400]
    });
    std::fs::write(java.join("config.txt"), json!({"ui": {
        "is-classic-mode": true, "extra-mode": 1, "show-comment": true,
        "custom-layout-1": snapshot, "custom-layout-2": {"show-subboard": true,
            "main-frame-maxsize": true}
    }}).to_string()).unwrap();
    std::fs::write(java.join("persist"), json!({"ui-persist": {
        "independent-main-board": [1, 2, 500, 500], "independent-sub-board": [3, 4, 300, 300]
    }}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for name in ["ui.custom-layout-1", "ui.custom-layout-2", "ui.is-classic-mode",
        "persist:ui-persist.independent-main-board", "persist:ui-persist.independent-sub-board"] {
        assert_eq!(item(&p.items, name).status, ImportStatus::Imported, "{name}");
    }
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let s = wb.view(false).settings;
    assert!(s.panels.classic_mode && !s.panels.show_comment && s.comment_panel_auto_hidden);
    assert_eq!(s.independent_main_board.unwrap().x, 1);
    wb.apply_custom_layout(1).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::FloatBoard);
    assert!(s.panels.show_comment && s.panels.showing_independent_main && s.panels.showing_independent_sub);
    assert!(!s.panels.show_sub_board && !s.panels.show_status && !s.panels.show_captured);
    assert_eq!(s.panel_sizes.board_position_proportion, 6);
    assert_eq!(s.independent_main_board.unwrap().x, 30);
    assert_eq!(s.independent_sub_board.unwrap().height, 400);
    drop(wb);
    let mut wb = workbench(&product, Recorder::default());
    assert_eq!(wb.view(false).settings.extra_mode, ExtraMode::FloatBoard);
    wb.apply_custom_layout(2).unwrap();
    let s = wb.view(false).settings;
    assert_eq!(s.extra_mode, ExtraMode::Normal);
    assert!(s.custom_layouts[1].as_ref().unwrap().window.unwrap().maximized);
    assert!(s.panels.show_sub_board && !s.panels.show_comment);
}

#[test]
fn human_engine_game_imports_java_dialog_values_and_reports_each_key() {
    use lizzie_core::settings::{AiTimeMode, KataTimeType};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    // Config.java:1668-2143,3554-3563; NewGameDialog.java:213-245,305-307.
    // Handicap is saved as a combo index, whereas the Rust setting is a stone count.
    let source = json!({"ui": {
        "check-play-black": false, "check-continue-play": true,
        "new-game-handicap": 8, "use-free-handicap": true, "new-game-komi": "0.5",
        "limit-my-time": true, "my-save-time": 7, "my-byoyomo-seconds": 6, "my-byoyomo-times": 5,
        "advance-time-settings": false, "advance-time-txt": "time_settings 30 5 1",
        "kata-time-settings": true, "kata-time-type": 1, "kata-time-main-time-mins": 25,
        "kata-time-byoyomi-secs": 30, "kata-time-byoyomi-times": 5,
        "kata-time-fisher-increment-secs": 8, "genmove-game-notime": false,
        "kata-visits-playouts-settings": true, "kata-visits-txt": 3000, "kata-playouts-txt": -2,
        "use-play-mode": false, "new-game-show-black": true, "new-game-show-white": true,
        "auto-save-played-game": false, "use-pure-net-in-game": true,
        "anagame-resign-start-move": 120, "anagame-resign-move": 4, "anagame-resign-percent": 2.5,
        "disable-wrn-in-game": false, "enable-anagame-randomstart": true,
        "anagame-random-move": 15, "anagame-random-winratediff": 1.25, "anagame-random-playoutdiff": 12.5,
        "new-engine-game-komi": 6.5, "pk-advance-time-settings": true
    }, "leelaz": {"play-ponder": false, "max-game-thinking-time-seconds": 9}});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let before = wb.config().game.clone();
    let p = preview(&mut wb, &java);
    assert_eq!(wb.config().game, before, "preview writes nothing");
    for section in ["ui", "leelaz"] {
        for key in source[section].as_object().unwrap().keys() {
            assert_eq!(item(&p.items, &format!("{section}.{key}")).status,
                if key.starts_with("new-engine-game-") || key.starts_with("pk-") {
                    ImportStatus::NotMigrated
                } else { ImportStatus::Imported }, "{section}.{key}");
        }
    }
    wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
    let g = &wb.config().game;
    assert!(!g.play_black && g.continue_play && g.free_handicap && g.limit_my_time);
    assert_eq!((g.handicap, g.komi), (9, 0.5));
    assert_eq!((g.my_save_time, g.my_byoyomi_seconds, g.my_byoyomi_times), (7, 6, 5));
    assert_eq!((g.save_minutes(), g.byoyomi_seconds(), g.byoyomi_times()), (180, 60, 20));
    assert_eq!(g.ai_time.mode, AiTimeMode::KataGo);
    assert_eq!(g.ai_time.kata_type, KataTimeType::Fischer);
    assert_eq!(g.ai_time.advanced, "time_settings 30 5 1");
    assert_eq!((g.ai_time.fixed_seconds, g.ai_time.kata_main_minutes, g.ai_time.kata_byoyomi_seconds,
        g.ai_time.kata_byoyomi_times, g.ai_time.kata_fischer_increment), (9, 25, 30, 5, 8));
    assert!(g.kata_limits && g.show_black && g.show_white && g.pure_net);
    assert!(!g.play_ponder && !g.use_play_mode && !g.auto_save && !g.disable_wrn);
    assert_eq!((g.kata_visits, g.kata_playouts), (3000, -2));
    assert_eq!((g.resign_start_move, g.resign_move, g.resign_percent), (120, 4, 2.5));
    assert!(g.random_start.enabled);
    assert_eq!((g.random_start.moves, g.random_start.winrate_diff, g.random_start.playouts_diff), (15, 1.25, 12.5));
    assert_eq!(&saved_config(&product).game, g);
}

#[test]
fn game_time_mode_uses_java_precedence_and_missing_keys_preserve_settings() {
    use lizzie_core::settings::AiTimeMode;
    // DesktopTimeControl.java:28-32: engineOwned > rawAdvanced > kataGoAdvanced > Fixed.
    for bits in 0..8 {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        let owned = bits & 1 != 0;
        let advanced = bits & 2 != 0;
        let kata = bits & 4 != 0;
        std::fs::write(java.join("config.txt"), json!({"ui": {
            "genmove-game-notime": owned, "advance-time-settings": advanced, "kata-time-settings": kata
        }}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(wb.config().game.ai_time.mode, if owned { AiTimeMode::EngineOwned }
            else if advanced { AiTimeMode::Advanced } else if kata { AiTimeMode::KataGo }
            else { AiTimeMode::Fixed }, "flags {bits}");
        let before = wb.config().game.clone();
        let autoplay = wb.persisted().autoplay;
        std::fs::write(java.join("config.txt"), json!({"ui": {}}).to_string()).unwrap();
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        assert_eq!(wb.config().game, before);
        assert_eq!(wb.persisted().autoplay, autoplay);
    }
}

#[test]
fn game_combo_indices_are_translated_or_left_unmapped_when_java_would_fail() {
    for index in 0..=9 {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({"ui": {
            "new-game-handicap": index, "my-save-time": 8, "my-byoyomo-seconds": -2,
            "my-byoyomo-times": 6, "kata-time-type": 3
        }}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let before = wb.config().game.clone();
        let p = preview(&mut wb, &java);
        for key in ["my-save-time", "my-byoyomo-seconds", "my-byoyomo-times", "kata-time-type"] {
            let row = item(&p.items, &format!("ui.{key}"));
            assert_eq!(row.status, ImportStatus::NotMigrated);
            assert_eq!(row.reason, Some(ImportReason::InvalidValue));
        }
        assert_eq!(item(&p.items, "ui.new-game-handicap").status,
            if index == 9 { ImportStatus::NotMigrated } else { ImportStatus::Imported });
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let g = &wb.config().game;
        assert_eq!(g.handicap, if index == 0 || index == 9 { 0 } else { index + 1 });
        assert_eq!((g.my_save_time, g.my_byoyomi_seconds, g.my_byoyomi_times, g.ai_time.kata_type),
            (before.my_save_time, before.my_byoyomi_seconds, before.my_byoyomi_times, before.ai_time.kata_type));
    }
}

#[test]
fn autoplay_imports_toolbar_slots_with_java_empty_and_checkbox_restore_semantics() {
    // Config.java:3307-3331 / BottomToolbar.java:2378-2379,2412-2434. Black, white and
    // time start checked and loading only sets them true, even for saved -1 / zero.
    for slots in [[1, 1, 1, 6, 1, 300, 1, 100], [-1; 8], [0; 8]] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({"ui": {}}).to_string()).unwrap();
        let mut toolbar = vec![json!(-1); 51];
        for (offset, value) in slots.into_iter().enumerate() { toolbar[9 + offset] = json!(value); }
        toolbar[40] = json!(3); toolbar[41] = json!(250);
        toolbar[47] = json!(true); toolbar[48] = json!(true);
        std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        for index in 9..=16 {
            let row = item(&p.items, &format!("persist:ui-persist.toolbar-parameter[{index}]"));
            assert_eq!(row.status, ImportStatus::Imported);
            assert_eq!(row.target, Some(ImportTarget::AutoPlay));
        }
        assert_eq!(item(&p.items, "persist:ui-persist.toolbar-parameter").status, ImportStatus::NotMigrated);
        wb.apply_java_import(p.id, &[], true, None).unwrap().unwrap();
        let a = wb.persisted().autoplay;
        assert!(a.black && a.white && a.time_enabled);
        let field = |n| if n > 0 { n } else { -1 };
        assert_eq!((a.time_seconds, a.playouts, a.first_playouts), (field(slots[3]), field(slots[5]), field(slots[7])));
        assert_eq!((a.playouts_enabled, a.first_playouts_enabled), (slots[4] > 0, slots[6] > 0));
        assert_eq!((wb.config().display.autoplay_main_seconds, wb.config().display.autoplay_sub_millis), (3.0, 250));
        drop(wb);
        assert_eq!(workbench(&product, Recorder::default()).persisted().autoplay, a, "persisted on import");
    }
}
