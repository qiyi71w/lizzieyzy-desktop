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
    Config, DisplaySettings, EstimateStyle, ExtraMode, FramePlacement, HawkeyeBranch, HawkeyePeriod, Language, MoveRankMode,
    Persisted, ProblemSide, ProblemSort, ProxyMode, Storage, SubBoardMode, WindowPlacement,
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
            "always-gotolast-onlive": true,
            "open-html-onlive": false,
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
            "auto-check-engine-alive": false,
            "always-gtp": false,
            "kata-rules": "japanese",
            "auto-load-kata-rules": true,
            "txt-kata-engine-pda": "0.5",
            "chk-kata-engine-pda": true,
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
            "enable-lizzie-cache": false,
            "stop-at-empty-board": true,
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
    // starting anything; the remote engine and the one with a token need confirmation.
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
    // The remote engine's command runs on its SSH host as written; its password needs consent.
    let remote = &p.engines[2];
    assert_eq!(
        (remote.status, remote.reason, remote.command.as_str()),
        (ImportStatus::Imported, None, "katago gtp")
    );
    assert!(remote.credential && remote.problems.is_empty());
    assert!(p.engines[3].credential);

    let report = wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    assert!(report.restart_required, "English applies after a restart");
    for left_out in [2, 3] {
        assert_eq!(
            (report.engines[left_out].status, report.engines[left_out].reason),
            (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed))
        );
    }
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
    assert!(!a.fast_engine_change && !a.show_ponder_limited_tips && !a.auto_check_engine_alive);
    assert!(!a.enable_lizzie_cache && a.stop_at_empty_board && !a.always_gtp);
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
    assert_eq!((c.engine.kata_pda, c.engine.kata_pda_enabled), (Some(0.5), true));
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
        (c.live.always_goto_last, c.live.open_html_on_live),
        (true, false),
        "both differ from the defaults"
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
    let report = wb.apply_java_import(p.id, &[3], false, false, false, None).unwrap().unwrap();
    assert_eq!(report.engines[3].status, ImportStatus::Imported);
    let c = saved_config(&product);
    let proxy = c.engine.engines.iter().find(|e| e.name == "Proxy").unwrap();
    assert!(
        proxy.command.ends_with(SECRETS[2]),
        "the confirmed command is kept as written"
    );
    // A preview is applied once.
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_STALE));
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
        wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(),
        Err(IMPORT_NEEDS_REPLACE)
    );
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), before);
    assert_eq!(wb.config().engine.engines[0].name, "Mine");

    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    assert!(wb.apply_java_import(p.id, &[], false, false, true, None).is_err());
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    assert!(wb.apply_java_import(p.id, &[], false, false, true, None).is_err());
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
        wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(),
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[2, 3], false, false, false, None).unwrap().unwrap();
    // The same Java settings, except the engine used last.
    let config = std::fs::read_to_string(java.join("config.txt")).unwrap();
    std::fs::write(
        java.join("config.txt"),
        config.replace("\"last-engine\": 0", "\"last-engine\": 1"),
    )
    .unwrap();
    let p = preview(&mut wb, &java);
    // The credentials are chosen after the preview: copying none of them would also leave out
    // the two engines carrying them.
    assert_eq!(p.replaces, [ImportTarget::LastEngine, ImportTarget::Engines]);
    for copy in [&[2, 3][..], &[]] {
        assert_eq!(
            wb.apply_java_import(p.id, copy, false, false, false, None).unwrap(),
            Err(IMPORT_NEEDS_REPLACE)
        );
    }
    assert_eq!(wb.config().engine.last_engine.as_deref(), Some("java-0"));
}

#[test]
fn engines_left_out_for_their_credentials_on_a_later_import_need_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "autoload-empty": true, "last-engine": 0 },
        "leelaz": { "engine-settings-list": [
            { "id": "proxy-1", "name": "Proxy", "command": format!("katago gtp --token {}", SECRETS[2]), "isDefault": false },
        ] },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    // The first import needs no replace confirmation; the credential copy is confirmed.
    let p = preview(&mut wb, &java);
    assert!(p.replaces.is_empty());
    wb.apply_java_import(p.id, &[0], false, false, false, None).unwrap().unwrap();
    let before = std::fs::read(storage(&product).config_file).unwrap();

    // The same Java settings again: leaving the credential unticked would remove the engine.
    let p = preview(&mut wb, &java);
    assert_eq!(p.replaces, [ImportTarget::Engines, ImportTarget::LastEngine]);
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), before);
    assert_eq!(wb.config().engine.engines.len(), 1);
    assert_eq!(wb.config().engine.last_engine.as_deref(), Some("proxy-1"));
    // Copying it again changes nothing, so no confirmation is needed.
    wb.apply_java_import(p.id, &[0], false, false, false, None).unwrap().unwrap();
    assert_eq!(wb.config().engine.engines.len(), 1);
}

#[test]
fn a_different_dismissed_startup_benchmark_prompt_needs_confirmation() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let write = |signature: &str, version: u32| {
        let ui = json!({ "ui": {
            "katago-startup-benchmark-dismissed-signature": signature,
            "katago-startup-benchmark-dismissed-version": version,
        } });
        std::fs::write(java.join("config.txt"), ui.to_string()).unwrap();
    };
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    write("sig-a", 1);
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    for (signature, version) in [("sig-b", 1), ("sig-a", 2)] {
        write(signature, version);
        let p = preview(&mut wb, &java);
        assert_eq!(p.replaces, [ImportTarget::StartupBenchmark], "{signature} {version}");
        assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_NEEDS_REPLACE));
        assert_eq!(wb.config().engine.startup_benchmark_dismissed_signature, "sig-a");
        assert_eq!(wb.config().engine.startup_benchmark_dismissed_version, 1);
    }
}

#[test]
fn after_resetting_the_interface_position_an_imported_picture_folder_is_kept() {
    // A Java profile that saved a picture but never opened a game record (`last-folder` "").
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({ "ui": {} }).to_string()).unwrap();
    let persist = json!({ "filesystem": { "last-folder": "", "last-image-folder": "/pictures" } });
    std::fs::write(java.join("persist"), persist.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.reset_ui_position().unwrap();
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "persist:filesystem.last-image-folder").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    drop(wb);
    let reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.persisted().last_image_folder.as_deref(), Some("/pictures"));
    assert_eq!(reopened.persisted().last_folder, None);
}

#[test]
fn a_placement_reset_before_an_import_does_not_come_back() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    std::fs::write(java.join("persist"), json!({"filesystem": {"last-folder": "/imported-games"}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let frame = FramePlacement { x: 11, y: 22, width: 333, height: 444, columns: vec![35, 20, 40, 70, 35, 50, 35] };
    let window = WindowPlacement { x: 5, y: 6, width: 900, height: 700, maximized: false };
    wb.persist_ui(Some(window), Some(frame), Some(vec![10, 30, 30, 45, 30])).unwrap();
    drop(wb);
    let mut wb = workbench(&product, Recorder::default());
    assert!(wb.persisted().suggestions_frame_7.is_some());
    wb.reset_ui_position().unwrap();
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    drop(wb);
    let reopened = workbench(&product, Recorder::default());
    let persisted = reopened.persisted();
    assert_eq!(persisted.last_folder.as_deref(), Some("/imported-games"));
    assert_eq!(persisted.window, None);
    assert_eq!(persisted.suggestions_frame_7, None);
    assert_eq!(persisted.main_list_columns, None);
}

#[test]
fn commands_carrying_unconfirmed_credentials_are_not_saved() {
    const MARKER: &str = "TEST_MARKER";
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let engine = format!("katago gtp --token {MARKER}");
    let config = json!({
        "ui": {
            "analysis-engine-command": format!("katago analysis --token {MARKER} -model m.bin.gz -config a.cfg"),
            "analysis-engine-command-customized": true,
            "engine-pk-black-commands": engine,
            "engine-pk-black-name": "Proxy",
            "engine-pk-white-commands": engine,
        },
        "leelaz": { "engine-settings-list": [
            { "id": "proxy-1", "name": "Proxy", "command": engine, "isDefault": false },
        ] },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    for copy in [&[][..], &[0]] {
        let product = tmp.path().join(format!("product-{}", copy.len()));
        let mut wb = workbench(&product, Recorder::default());
        let analysis_before = wb.config().analysis_engine.clone();
        let p = preview(&mut wb, &java);
        assert!(!serde_json::to_string(&p).unwrap().contains(MARKER));
        let report = wb.apply_java_import(p.id, copy, false, false, true, None).unwrap().unwrap();
        assert!(!serde_json::to_string(&report).unwrap().contains(MARKER));
        // 精析设置 has no confirmation of its own: its command and flag stay this product's.
        assert_eq!(wb.config().analysis_engine.command, analysis_before.command);
        assert_eq!(wb.config().analysis_engine.command_customized, analysis_before.command_customized);
        for key in ["ui.analysis-engine-command", "ui.analysis-engine-command-customized"] {
            let it = item(&report.items, key);
            assert_eq!((it.status, it.reason), (ImportStatus::NotMigrated, Some(ImportReason::Credential)), "{key}");
        }
        // A side's command comes over with the engine it names, so only with its credentials.
        let sides = &wb.config().matches;
        let saved = std::fs::read_to_string(storage(&product).config_file).unwrap();
        if copy.is_empty() {
            assert!(!saved.contains(MARKER), "nothing confirmed, nothing saved");
            assert_eq!((sides.first.commands.as_str(), sides.second.commands.as_str()), ("", ""));
            for key in ["ui.engine-pk-black-commands", "ui.engine-pk-white-commands"] {
                let it = item(&report.items, key);
                assert_eq!((it.status, it.reason), (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed)), "{key}");
            }
        } else {
            assert_eq!((sides.first.commands.as_str(), sides.second.commands.as_str()), (engine.as_str(), engine.as_str()));
            assert_eq!(item(&report.items, "ui.engine-pk-black-commands").status, ImportStatus::Imported);
        }
        assert_eq!(sides.first.name, "Proxy");
    }
}

#[test]
fn analysis_and_match_commands_without_credentials_still_come_over() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({ "ui": {
        "analysis-engine-command": "katago analysis -model /m.bin.gz -config /a.cfg",
        "analysis-engine-command-customized": true,
        "engine-pk-black-commands": "katago gtp -model /m.bin.gz",
        "engine-pk-white-commands": "leelaz --gtp",
    } });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    let report = wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    for key in ["ui.analysis-engine-command", "ui.engine-pk-black-commands", "ui.engine-pk-white-commands"] {
        assert_eq!(item(&report.items, key).status, ImportStatus::Imported, "{key}");
    }
    let c = wb.config();
    assert_eq!(c.analysis_engine.command, "katago analysis -model /m.bin.gz -config /a.cfg");
    assert!(c.analysis_engine.command_customized);
    assert_eq!(c.matches.first.commands, "katago gtp -model /m.bin.gz");
    assert_eq!(c.matches.second.commands, "leelaz --gtp");
}

#[test]
fn a_board_size_this_product_does_not_open_is_reported_and_left_alone() {
    // Java's 综合设置 saves 53 (ConfigDialog2 only corrects values below 2).
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({ "ui": { "other-size-width": 53, "other-size-height": 13 } });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let before = wb.config().edit.other_size_width;
    let p = preview(&mut wb, &java);
    let report = wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    for items in [&p.items, &report.items] {
        let width = item(items, "ui.other-size-width");
        assert_eq!((width.status, width.reason), (ImportStatus::Incompatible, Some(ImportReason::InvalidValue)));
        assert_eq!(item(items, "ui.other-size-height").status, ImportStatus::Imported);
    }
    assert_eq!(wb.config().edit.other_size_width, before);
    assert_eq!(wb.config().edit.other_size_height, 13);
}

#[test]
fn java_migration_markers_are_reported_once_as_java_state() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({ "ui": {
        "show-blunder-bar": true,
        "migrated-hide-blunder-bar-default-v1": true,
        "migrated-windows-portable-user-state-v2": true,
    } });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    let report = wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    for items in [&p.items, &report.items] {
        for key in ["ui.migrated-hide-blunder-bar-default-v1", "ui.migrated-windows-portable-user-state-v2"] {
            let rows: Vec<_> = items.iter().filter(|i| i.key == key).map(|i| (i.status, i.reason)).collect();
            assert_eq!(rows, [(ImportStatus::NotMigrated, Some(ImportReason::JavaState))], "{key}");
        }
    }
    // The marker still decides how the blunder bar comes over.
    assert!(wb.config().display.blunder_bar);
}

#[test]
fn an_unchecked_auto_replay_box_turns_the_setting_off_unless_java_starts_it_anyway() {
    // Java saves -1 for the unchecked 自动播放分支 box (Config.java:3395-3396); an explicit
    // `ui.auto-replay-branch` still starts it (Lizzie.java:488).
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let toolbar = vec![json!(-1); 51];
    std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
    for (ui, expected) in [(json!({}), false), (json!({"auto-replay-branch": true}), true)] {
        std::fs::write(java.join("config.txt"), json!({ "ui": ui }).to_string()).unwrap();
        let product = tmp.path().join(format!("product-{expected}"));
        let mut wb = workbench(&product, Recorder::default());
        let mut display = wb.config().display.clone();
        display.auto_replay_branch = !expected;
        wb.set_display_settings(display).unwrap();
        let p = preview(&mut wb, &java);
        let report = wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        for items in [&p.items, &report.items] {
            let slot = item(items, "persist:ui-persist.toolbar-parameter[39]");
            assert_eq!((slot.status, slot.target), (ImportStatus::Imported, Some(ImportTarget::Suggestions)), "{ui}");
        }
        drop(wb);
        assert_eq!(workbench(&product, Recorder::default()).config().display.auto_replay_branch, expected, "{ui}");
    }
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
            { "name": "Remote", "command": "katago gtp", "useJavaSSH": true, "ip": "h", "userName": "u", "password": "SSH-SECRET" },
        ] },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.last-engine").status, ImportStatus::Imported);
    let report = wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    for key in ["ui.default-engine", "ui.last-engine"] {
        let choice = item(&report.items, key);
        assert_eq!(
            (choice.status, choice.reason),
            (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed)),
            "{key}"
        );
    }
    assert!(wb.config().engine.engines.is_empty() && wb.config().engine.last_engine.is_none());
}

/// `Utils.doEncrypt("p@ss w0rd/围棋")` as the fixed Java writes it (run against its
/// `Base64AesEncrypt`, trailing CRLF included).
const JAVA_ENCRYPTED: &str = "S2rB+6fIpljxobY0+9mBZ0zHZ66UlgIBYUIOewQf49Q=\r\n";

#[test]
fn remote_engines_and_their_passwords_come_over_only_when_confirmed() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "analysis-engine-command": "katago analysis -model /remote/m.bin.gz -config /remote/a.cfg" },
        "leelaz": {
            "analyze-update-interval-centisecssh": 25,
            "analysis-engine-ssh-info": {
                "useJavaSSH": true, "ip": "10.0.0.9", "port": "2222", "userName": "go",
                "password": JAVA_ENCRYPTED, "useKeyGen": false, "keyGenPath": "",
            },
            "engine-settings-list": [
                { "name": "Password", "command": "/remote/katago gtp -model /remote/m.bin.gz", "useJavaSSH": true,
                  "ip": "10.0.0.1", "port": "x", "userName": "go", "password": JAVA_ENCRYPTED },
                { "name": "Key", "command": "katago gtp", "useJavaSSH": true, "ip": "10.0.0.2", "port": "22",
                  "userName": "go", "useKeyGen": true, "keyGenPath": "/nonexistent/id_ed25519", "password": JAVA_ENCRYPTED },
            ],
        },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    assert!(!serde_json::to_string(&p).unwrap().contains("p@ss"));
    // Remote paths are the host's: not resolved or checked here. A key login needs no password,
    // but its key file must exist on this machine.
    assert!(p.engines[0].credential && p.engines[0].problems.is_empty());
    assert_eq!(p.engines[0].command, "/remote/katago gtp -model /remote/m.bin.gz");
    assert!(!p.engines[1].credential);
    assert!(matches!(&p.engines[1].problems[..], [EngineProblem::FileNotFound { path, .. }] if path == "/nonexistent/id_ed25519"));
    assert_eq!(p.analysis_ssh_credential.as_deref(), Some("go@10.0.0.9:2222"));

    // Only the confirmed passwords are handed to the credential store, decrypted like Java.
    assert!(wb.java_ssh_passwords(p.id, &[], false, false).is_empty());
    assert_eq!(
        wb.java_ssh_passwords(p.id, &[0, 1], true, false),
        [
            ("go@10.0.0.1:22".to_string(), "p@ss w0rd/围棋".to_string()),
            ("go@10.0.0.9:2222".to_string(), "p@ss w0rd/围棋".to_string()),
        ]
    );
    let report = wb.apply_java_import(p.id, &[0], false, false, false, None).unwrap().unwrap();
    assert_eq!(report.engines[0].status, ImportStatus::Imported);
    let c = wb.config();
    let ssh = c.engine.engines[0].ssh.as_ref().expect("remote engine");
    assert_eq!((ssh.host.as_str(), ssh.port.as_str(), ssh.user.as_str(), ssh.use_key), ("10.0.0.1", "x", "go", false));
    assert_eq!(c.engine.engines[0].command, "/remote/katago gtp -model /remote/m.bin.gz");
    assert!(c.analysis_engine.use_ssh && c.analysis_engine.ssh.account() == "go@10.0.0.9:2222");
    assert_eq!(c.analysis_engine.command, "katago analysis -model /remote/m.bin.gz -config /remote/a.cfg");
    assert_eq!(c.engine.analysis.interval_centisec_ssh, 25);
    let saved = std::fs::read_to_string(storage(&product).config_file).unwrap();
    assert!(!saved.contains("p@ss") && !saved.contains(JAVA_ENCRYPTED.trim()));
}

/// The Java 跑谱贡献 settings (`ContributeSettings`, `ContributeView`) with an account, a custom
/// command carrying a password and a 远程SSH password.
fn contribution_java(root: &Path) -> PathBuf {
    let java = root.join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": {
            "contribute-engine-path": "/opt/katago",
            "contribute-config-path": "contribute.cfg",
            "contribute-user-name": "trainer",
            "contribute-password": "TRAIN-SECRET",
            "contribute-batch-games": "12",
            "contribute-show-estimate": true,
            "contribute-disable-rating-matches": true,
            "contribute-use-command": true,
            "contribute-command": "katago contribute -config c.cfg -override-config password=CMD-SECRET",
            "contribute-use-pure-command": false,
            "contribute-auto-save": true,
            "contribute-watch-auto-play": false,
            "contribute-watch-auto-play-interval": 3.5,
            "contribute-watch-skip-none-19": true,
            "contribute-always-top": false,
            "show-Contribute": false,
        },
        "leelaz": {
            "contribute-engine-ssh-info": {
                "useJavaSSH": true, "ip": "10.0.0.7", "port": "2200", "userName": "go",
                "password": JAVA_ENCRYPTED, "useKeyGen": false, "keyGenPath": "",
            },
        },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    java
}

#[test]
fn contribution_settings_come_over_and_its_account_only_when_confirmed() {
    let tmp = tempfile::tempdir().unwrap();
    let java = contribution_java(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    let shown = serde_json::to_string(&p).unwrap();
    for secret in ["trainer", "TRAIN-SECRET", "CMD-SECRET", "p@ss"] {
        assert!(!shown.contains(secret), "{secret} in the preview");
    }
    assert!(p.contribution_credential);
    for key in ["ui.contribute-user-name", "ui.contribute-password", "ui.contribute-command"] {
        let it = item(&p.items, key);
        assert!(it.credential && it.value.is_none(), "{key}");
    }
    assert_eq!(item(&p.items, "ui.contribute-batch-games").target, Some(ImportTarget::Contribution));

    // Not confirmed: the settings come over, the account, its password and the command do not.
    assert!(wb.java_contribution_credentials(p.id, false).is_none());
    assert!(wb.java_ssh_passwords(p.id, &[], false, false).is_empty());
    let report = wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    for key in ["ui.contribute-user-name", "ui.contribute-password", "ui.contribute-command"] {
        let it = item(&report.items, key);
        assert_eq!((it.status, it.reason), (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed)), "{key}");
    }
    let c = saved_config(&product).contribution;
    assert_eq!((c.engine_path.as_str(), c.config_path.as_str(), c.batch_games.as_str()), ("/opt/katago", "contribute.cfg", "12"));
    assert!(c.show_estimate && c.disable_rating_matches && c.use_command && c.auto_save && !c.show_menu);
    assert!(!c.watch.auto_play && c.watch.skip_non19 && !c.watch.always_top && c.watch.auto_play_interval == 3.5);
    assert!(c.remote && c.ssh.account() == "go@10.0.0.7:2200");
    assert_eq!((c.user_name.as_str(), c.command.as_str()), ("", ""));

    // Confirmed: the account, the command concealed as saved, and the passwords for the store.
    let tmp = tempfile::tempdir().unwrap();
    let java = contribution_java(tmp.path());
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    let credentials = wb.java_contribution_credentials(p.id, true).unwrap();
    assert_eq!((credentials.user_name.as_str(), credentials.password.as_str()), ("trainer", "TRAIN-SECRET"));
    assert_eq!(credentials.command.as_deref(), Some("katago contribute -config c.cfg -override-config password=CMD-SECRET"));
    assert_eq!(wb.java_ssh_passwords(p.id, &[], false, true), [("go@10.0.0.7:2200".to_string(), "p@ss w0rd/围棋".to_string())]);
    let report = wb.apply_java_import(p.id, &[], true, false, false, None).unwrap().unwrap();
    assert_eq!(item(&report.items, "ui.contribute-user-name").status, ImportStatus::Imported);
    let c = saved_config(&product).contribution;
    assert_eq!(c.user_name, "trainer");
    assert_eq!(c.command, "katago contribute -config c.cfg -override-config password=******");
    let saved = std::fs::read_to_string(storage(&product).config_file).unwrap();
    for secret in ["TRAIN-SECRET", "CMD-SECRET", "p@ss", JAVA_ENCRYPTED.trim()] {
        assert!(!saved.contains(secret), "{secret} saved");
    }
}

#[test]
fn java_keys_without_a_setting_say_why() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let config = json!({
        "ui": { "first-time-load": false, "use-factor": true, "always-gtp": "sometimes" },
        "leelaz": { "engine-settings-list": [], "engine-command": "old-katago gtp" },
    });
    std::fs::write(java.join("config.txt"), config.to_string()).unwrap();
    let persist = json!({ "ui-persist": { "gtp-console-position": [1, 2, 3, 4] }, "first-time-factor": false });
    std::fs::write(java.join("persist"), persist.to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    let reason = |key: &str| (item(&p.items, key).status, item(&p.items, key).reason);
    // Java reads the older engine keys only without `engine-settings-list`.
    assert_eq!(reason("leelaz.engine-command"), (ImportStatus::NotMigrated, Some(ImportReason::Obsolete)));
    assert_eq!(reason("ui.use-factor"), (ImportStatus::NotMigrated, Some(ImportReason::Obsolete)));
    assert_eq!(reason("ui.first-time-load"), (ImportStatus::NotMigrated, Some(ImportReason::JavaState)));
    assert_eq!(reason("persist:first-time-factor"), (ImportStatus::NotMigrated, Some(ImportReason::JavaState)));
    assert_eq!(
        reason("persist:ui-persist.gtp-console-position"),
        (ImportStatus::Incompatible, Some(ImportReason::WindowCoordinates))
    );
    // A value Java's optBoolean would not read is reported, not guessed.
    assert_eq!(reason("ui.always-gtp"), (ImportStatus::Incompatible, Some(ImportReason::InvalidValue)));
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
                "disable-move-rank-in-origin": true,
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
                disable_move_rank_in_origin: true,
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        // Java's migration marker itself is its own bookkeeping, reported as such.
        assert!(p.items.iter().all(|item| item.status == ImportStatus::Imported
            || (item.key == "ui.migrated-hide-blunder-bar-default-v1" && item.reason == Some(ImportReason::JavaState))), "{ui}");
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        assert_eq!(wb.config().whole_game.deep_visits, expected);
        assert_eq!(saved_config(&product).whole_game.deep_visits, expected);
    }
}

#[test]
fn web_board_ports_and_viewer_limit_import_like_java_starts_with_them() {
    // WebBoardManager.start: top-level `web-board`, optInt with 9998 / 9999 / 20.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {}, "web-board": {"http-port": "8080", "ws-port": 70000, "max-connections": 3}});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for (key, status) in [
        ("web-board.http-port", ImportStatus::Imported),
        ("web-board.ws-port", ImportStatus::Incompatible),
        ("web-board.max-connections", ImportStatus::Imported),
    ] {
        let row = item(&p.items, key);
        assert_eq!((row.status, row.target), (status, Some(ImportTarget::WebBoard)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let w = saved_config(&product).web_board;
    assert_eq!((w.http_port, w.ws_port, w.max_connections), (8080, 9999, 3));
}

#[test]
fn diagnostics_settings_import_by_value_and_unknown_names_are_invalid_alone() {
    // LoggingSettings.fromJson: top-level `logging`, wire names; fromWireName rejects others.
    use lizzie_core::diagnostics::{DiagnosticModule, TraceScope};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {}, "logging": {
        "diagnostics-enabled": false,
        "diagnostic-modules": ["network-remote", "engine"],
        "preferred-full-trace-scopes": ["readboard-yike", "future-scope"],
    }});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for (key, status) in [
        ("logging.diagnostics-enabled", ImportStatus::Imported),
        ("logging.diagnostic-modules", ImportStatus::Imported),
        ("logging.preferred-full-trace-scopes", ImportStatus::Incompatible),
    ] {
        let row = item(&p.items, key);
        assert_eq!((row.status, row.target), (status, Some(ImportTarget::Diagnostics)), "{key}");
    }
    assert_eq!(item(&p.items, "logging.preferred-full-trace-scopes").reason, Some(ImportReason::InvalidValue));
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let l = saved_config(&product).logging;
    assert!(!l.diagnostics_enabled);
    assert_eq!(l.diagnostic_modules, [DiagnosticModule::Engine, DiagnosticModule::NetworkRemote]);
    assert_eq!(l.preferred_full_trace_scopes, TraceScope::ALL);
    assert_eq!(wb.view(false).settings.logging, l);
}

#[test]
fn katago_parameter_keys_import_like_java_reads_them() {
    // Config.java:1999-2015: booleans by optBoolean, values as text (empty is no value).
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {
        "chk-kata-engine-wrn": true, "txt-kata-engine-wrn": "0.3",
        "autoload-kata-engine-pda": true, "auto-load-txt-kata-engine-pda": "-0.5",
        "autoload-kata-engine-wrn": false, "auto-load-txt-kata-engine-wrn": "",
        "show-pda-in-menu": false, "show-wrn-in-menu": "nope",
    }});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for (key, status, target) in [
        ("ui.chk-kata-engine-wrn", ImportStatus::Imported, ImportTarget::KataWrn),
        ("ui.txt-kata-engine-wrn", ImportStatus::Imported, ImportTarget::KataWrn),
        ("ui.autoload-kata-engine-pda", ImportStatus::Imported, ImportTarget::KataParamAutoLoad),
        ("ui.auto-load-txt-kata-engine-pda", ImportStatus::Imported, ImportTarget::KataParamAutoLoad),
        ("ui.autoload-kata-engine-wrn", ImportStatus::Imported, ImportTarget::KataParamAutoLoad),
        ("ui.auto-load-txt-kata-engine-wrn", ImportStatus::Imported, ImportTarget::KataParamAutoLoad),
        ("ui.show-pda-in-menu", ImportStatus::Imported, ImportTarget::KataParamToolbar),
        ("ui.show-wrn-in-menu", ImportStatus::Incompatible, ImportTarget::KataParamToolbar),
    ] {
        let row = item(&p.items, key);
        assert_eq!((row.status, row.target), (status, Some(target)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let k = saved_config(&product).engine.kata_params;
    assert_eq!((k.wrn_enabled, k.wrn), (true, Some(0.3)));
    assert_eq!((k.auto_load_pda, k.auto_load_pda_value), (true, Some(-0.5)));
    assert_eq!((k.auto_load_wrn, k.auto_load_wrn_value), (false, None));
    assert_eq!((k.show_pda, k.show_wrn), (false, true), "an unreadable flag keeps Java's default");
}

#[test]
fn leela_option_keys_import_like_java_reads_them() {
    // Config.java:881-895, 1983-1997: optBoolean flags (note `autoload-Lzsai-…`) and the values
    // as text, kept as they are.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {
        "chk-lzsai-enginemem": true, "autoload-Lzsai-enginemem": true, "txt-lzsai-enginemem": "4096",
        "chk-lzsai-enginevisits": false, "autoload-Lzsai-enginevisits": true, "txt-lzsai-enginevisits": "800",
        "chk-lzsai-enginelagbuffer": true, "autoload-Lzsai-enginelagbuffer": false, "txt-lzsai-enginelagbuffer": "",
        "chk-lzsai-engineresign": "nope", "autoload-Lzsai-engineresign": true, "txt-lzsai-engineresign": "abc",
    }});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for row in ["mem", "visits", "lagbuffer", "resign"] {
        for key in [format!("ui.chk-lzsai-engine{row}"), format!("ui.autoload-Lzsai-engine{row}"), format!("ui.txt-lzsai-engine{row}")] {
            let item = item(&p.items, &key);
            let status = if key == "ui.chk-lzsai-engineresign" { ImportStatus::Incompatible } else { ImportStatus::Imported };
            assert_eq!((item.status, item.target), (status, Some(ImportTarget::LeelaOptions)), "{key}");
        }
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let o = saved_config(&product).engine.leela_options;
    assert_eq!((o.memory.enabled, o.memory.auto_load, o.memory.value.as_str()), (true, true, "4096"));
    assert_eq!((o.visits.enabled, o.visits.auto_load, o.visits.value.as_str()), (false, true, "800"));
    assert_eq!((o.lagbuffer.enabled, o.lagbuffer.auto_load, o.lagbuffer.value.as_str()), (true, false, ""));
    assert_eq!((o.resign.enabled, o.resign.auto_load, o.resign.value.as_str()), (false, true, "abc"), "an unreadable flag keeps Java's default");
}

#[test]
fn custom_pda_import_restores_modes_and_reports_invalid_commands_without_storing_them() {
    use lizzie_core::engine::profile::CustomPdaMode;
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let source = json!({"ui": {
        "chk-dym-pda": true, "chk-static-pda": false, "chk-auto-pda": true,
        "dym-pda-cap": "0.75", "static-pda-cur": "-1.5", "auto-pda": "dympdacap 0.75"
    }});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let p = preview(&mut wb, &java);
    for key in ["chk-dym-pda", "chk-static-pda", "chk-auto-pda", "dym-pda-cap", "static-pda-cur", "auto-pda"] {
        let row = item(&p.items, &format!("ui.{key}"));
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::CustomPda)));
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let saved = saved_config(&product).engine.custom_pda;
    assert_eq!(saved.mode, CustomPdaMode::Dynamic);
    assert_eq!((saved.dynamic_cap, saved.fixed_value, saved.auto_apply), (Some(0.75), Some(-1.5), true));
    let mut reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.config().engine.custom_pda, saved);

    // Invalid source values cannot destroy the already committed preference.
    std::fs::write(java.join("config.txt"), json!({"ui": {
        "dym-pda-cap": "NaN", "auto-pda": "pda 3\nquit"
    }}).to_string()).unwrap();
    let p = preview(&mut reopened, &java);
    for key in ["ui.dym-pda-cap", "ui.auto-pda"] {
        assert_eq!(item(&p.items, key).reason, Some(ImportReason::InvalidValue));
    }
    reopened.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(reopened.config().engine.custom_pda, saved);

    // A partial historical profile can recover the typed setting from its command.
    std::fs::write(java.join("config.txt"), json!({"ui": {"auto-pda": "pda -2"}}).to_string()).unwrap();
    let p = preview(&mut reopened, &java);
    assert!(p.replaces.contains(&ImportTarget::CustomPda));
    reopened.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(reopened.config().engine.custom_pda.mode, CustomPdaMode::Fixed);
    assert_eq!(reopened.config().engine.custom_pda.fixed_value, Some(-2.0));
    assert!(!serde_json::to_string(reopened.config()).unwrap().contains("pda -2"));
    // Disabled selection and a dormant zero coefficient are valid Java dialog state.
    std::fs::write(java.join("config.txt"), json!({"ui": {
        "chk-dym-pda": false, "chk-static-pda": false, "chk-auto-pda": false,
        "dym-pda-cap": "0", "static-pda-cur": "", "auto-pda": "pda 0"
    }}).to_string()).unwrap();
    let p = preview(&mut reopened, &java);
    assert_eq!(item(&p.items, "ui.dym-pda-cap").status, ImportStatus::Imported);
    reopened.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let saved = &reopened.config().engine.custom_pda;
    assert_eq!((saved.mode, saved.dynamic_cap, saved.fixed_value, saved.auto_apply),
        (CustomPdaMode::Disabled, Some(0.0), None, false));
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
    assert!(!p.items.iter().any(|i| i.key.starts_with("file:theme")), "a theme/ folder without theme folders holds no theme");
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
fn importing_a_bundled_theme_selects_it_and_skips_folders_without_theme_txt() {
    let tmp=tempfile::tempdir().unwrap(); let java=tmp.path().join("java");
    std::fs::create_dir_all(java.join("theme/Personal")).unwrap();
    std::fs::write(java.join("config.txt"),json!({"ui":{"theme":"sabaki"}}).to_string()).unwrap();
    let mut wb=workbench(&tmp.path().join("product"),Recorder::default());
    let p=preview(&mut wb,&java);
    assert!(!p.items.iter().any(|i| i.key.starts_with("file:theme")));
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let v=wb.view(true).settings;
    assert_eq!(v.appearance.theme,"sabaki"); assert_eq!(v.theme.shadow_size,100);
    assert_eq!(v.theme.stone_indicator_type,0);
    assert!(!lizzie_core::appearance::themes_dir(wb.storage()).join("Personal").exists());
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
    let theme=item(&p.items,"ui.theme");
    assert_eq!((theme.status,theme.reason),(ImportStatus::Incompatible,Some(ImportReason::Unreadable)));
    assert_eq!(item(&p.items,"ui.custom-board-image").reason,Some(ImportReason::Unreadable));
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(),Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(wb.view(false).settings.theme.font_name,"Lizzie Default");
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let s=wb.view(false).settings;
    assert_eq!(s.theme.font_name,"Imported Font"); assert!(s.appearance.custom_board_image.is_empty());
    assert_eq!(s.appearance.theme,"default");
}

const PLAIN_BOARD: &[u8] = include_bytes!("../../../ui/public/assets/board.png");
const SABAKI_BOARD: &[u8] = include_bytes!("../../../ui/public/themes/sabaki/board.png");

/// A Java theme folder as Java's 新建主题 writes it, with `board.png` given and `extra` values.
fn java_theme(java: &Path, name: &str, board: &[u8], extra: serde_json::Value) {
    let dir = java.join("theme").join(name);
    std::fs::create_dir_all(&dir).unwrap();
    let mut raw: serde_json::Map<String, serde_json::Value> =
        serde_json::from_slice(include_bytes!("../../../ui/public/assets/newtheme/theme.txt")).unwrap();
    raw.extend(extra.as_object().unwrap().clone());
    std::fs::write(dir.join("theme.txt"), serde_json::to_vec(&raw).unwrap()).unwrap();
    std::fs::write(dir.join("board.png"), board).unwrap();
    std::fs::write(dir.join("black.png"), include_bytes!("../../../ui/public/assets/newtheme/black.png")).unwrap();
    std::fs::write(dir.join("white.png"), include_bytes!("../../../ui/public/assets/newtheme/white.png")).unwrap();
}

fn data_url(bytes: &[u8], dir: &Path) -> String {
    let path = dir.join("expected.png");
    std::fs::write(&path, bytes).unwrap();
    lizzie_core::appearance::image_data(&path).unwrap()
}

/// The board image this product shows with theme `name` selected.
fn shown_board(wb: &Workbench, name: &str) -> String {
    let appearance = lizzie_core::appearance::AppearanceSettings { theme: name.into(), ..Default::default() };
    let theme = lizzie_core::appearance::load_theme(wb.storage(), name, &lizzie_core::appearance::ThemeSettings::default());
    lizzie_core::appearance::images(wb.storage(), &appearance, &theme).board
}

fn themes_snapshot(wb: &Workbench) -> Vec<(PathBuf, Vec<u8>)> {
    let dir = lizzie_core::appearance::themes_dir(wb.storage());
    if dir.exists() { snapshot(&dir) } else { Vec::new() }
}

#[test]
fn java_theme_folders_become_themes_with_their_own_images_and_stay_selected() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "我的主题", PLAIN_BOARD, json!({"shadow-size": 42, "winrate-line-color": [1, 2, 3]}));
    java_theme(&java, "Night", SABAKI_BOARD, json!({"background-image": "/no/such/file.png"}));
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "我的主题"}}).to_string()).unwrap();
    let java_before = snapshot(&java);
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ["file:theme/我的主题", "file:theme/Night"] {
        let it = item(&p.items, key);
        assert_eq!((it.status, it.target), (ImportStatus::Imported, Some(ImportTarget::Themes)), "{key}");
    }
    assert_eq!(item(&p.items, "ui.theme").status, ImportStatus::Imported);
    assert!(!p.replaces.contains(&ImportTarget::Themes) && p.replaced_themes.is_empty());
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let s = wb.view(true).settings;
    assert_eq!(s.appearance.theme, "我的主题");
    assert_eq!((s.theme.shadow_size, s.theme.winrate_line_color.clone()), (42, vec![1, 2, 3]));
    let names: Vec<String> = lizzie_core::appearance::catalog(wb.storage(), &Default::default()).into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"我的主题".to_string()) && names.contains(&"Night".to_string()), "{names:?}");
    // Same file name, different pictures: each theme shows its own.
    assert_eq!(shown_board(&wb, "我的主题"), data_url(PLAIN_BOARD, tmp.path()));
    assert_eq!(shown_board(&wb, "Night"), data_url(SABAKI_BOARD, tmp.path()));
    // Java takes an unreadable theme image from the ui settings; so does the imported theme.
    let night = lizzie_core::appearance::load_theme(wb.storage(), "Night", &Default::default());
    assert!(night.background_image.is_empty());
    assert_eq!(snapshot(&java), java_before, "the Java folder is only read");
    // Importing the same folders again replaces nothing.
    let again = preview(&mut wb, &java);
    assert!(!again.replaces.contains(&ImportTarget::Themes));
    drop(wb);
    let mut reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.view(false).settings.appearance.theme, "我的主题");
    assert_eq!(shown_board(&reopened, "我的主题"), data_url(PLAIN_BOARD, tmp.path()));
}

#[test]
fn java_bundled_theme_copy_is_skipped_and_a_changed_one_overrides_the_bundle() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/public/themes");
    for name in ["sabaki", "Fast", "Megapack"] {
        std::fs::create_dir_all(java.join("theme").join(name)).unwrap();
        for e in std::fs::read_dir(bundled.join(name)).unwrap() {
            let p = e.unwrap().path();
            std::fs::copy(&p, java.join("theme").join(name).join(p.file_name().unwrap())).unwrap();
        }
    }
    let fast = java.join("theme/Fast/theme.txt");
    let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(&fast).unwrap()).unwrap();
    raw["shadow-size"] = json!(37);
    std::fs::write(&fast, raw.to_string()).unwrap();
    std::fs::write(java.join("theme/Fast/board.jpg"), PLAIN_BOARD).unwrap();
    // Only a picture changed: still the user's own copy.
    std::fs::write(java.join("theme/Megapack/board.png"), SABAKI_BOARD).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "Fast"}}).to_string()).unwrap();
    let bundled_before = snapshot(&bundled);
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    assert!(!p.items.iter().any(|i| i.key == "file:theme/sabaki"), "an untouched bundled copy needs no folder");
    assert_eq!(item(&p.items, "file:theme/Fast").status, ImportStatus::Imported);
    assert_eq!(item(&p.items, "file:theme/Megapack").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let dir = lizzie_core::appearance::themes_dir(wb.storage());
    assert!(!dir.join("sabaki").exists() && dir.join("Fast/theme.txt").is_file());
    let s = wb.view(false).settings;
    assert_eq!((s.appearance.theme.as_str(), s.theme.shadow_size), ("Fast", 37));
    assert_eq!(shown_board(&wb, "Fast"), data_url(PLAIN_BOARD, tmp.path()));
    assert_eq!(shown_board(&wb, "Megapack"), data_url(SABAKI_BOARD, tmp.path()));
    assert_eq!(snapshot(&bundled), bundled_before, "the bundled files are not changed");
}

/// The board this product shows with its committed appearance.
fn committed_board(wb: &mut Workbench) -> String {
    let s = wb.view(false).settings;
    lizzie_core::appearance::images(wb.storage(), &s.appearance, &s.theme).board
}

#[test]
fn bundled_theme_missing_its_picture_shows_the_ui_picture_as_java_does() {
    // Theme.getImageByKey: the theme's board.png is unreadable, so Java reads ui `board-image`.
    let bundled = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../ui/public/themes/sabaki");
    for changed in [false, true] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        let dir = java.join("theme/sabaki");
        std::fs::create_dir_all(&dir).unwrap();
        for e in std::fs::read_dir(&bundled).unwrap() {
            let p = e.unwrap().path();
            if p.file_name().unwrap() != "board.png" { std::fs::copy(&p, dir.join(p.file_name().unwrap())).unwrap(); }
        }
        if changed {
            let mut raw: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("theme.txt")).unwrap()).unwrap();
            raw["shadow-size"] = json!(44);
            std::fs::write(dir.join("theme.txt"), raw.to_string()).unwrap();
        }
        std::fs::write(java.join("theme/ui-board.png"), PLAIN_BOARD).unwrap();
        std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "sabaki", "board-image": "ui-board.png"}}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        assert_eq!(committed_board(&mut wb), data_url(PLAIN_BOARD, tmp.path()), "theme.txt changed: {changed}");
        drop(wb);
        let mut reopened = workbench(&product, Recorder::default());
        assert_eq!(committed_board(&mut reopened), data_url(PLAIN_BOARD, tmp.path()), "after restart, changed: {changed}");
    }
}

#[test]
fn java_themes_come_from_the_work_or_program_folder_only() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {}}).to_string()).unwrap();
    let program = tmp.path().join("install/app");
    std::fs::create_dir_all(&program).unwrap();
    java_theme(&tmp.path().join("install"), "Elsewhere", PLAIN_BOARD, json!({}));
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let read = |wb: &mut Workbench| wb.preview_java_import(java_import::read(&JavaSource::new(&java, Some(&program))).unwrap());
    let p = read(&mut wb);
    assert!(!p.items.iter().any(|i| i.key.starts_with("file:theme")), "a parent of the program folder is not Java's theme folder");
    java_theme(&program, "Installed", PLAIN_BOARD, json!({}));
    let p = read(&mut wb);
    assert_eq!(item(&p.items, "file:theme/Installed").status, ImportStatus::Imported);
}

#[test]
fn user_theme_without_its_picture_shows_java_s_shared_theme_folder_picture() {
    // No board.png in the theme and no ui `board-image`: Java reads theme/board.png.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "Mine", PLAIN_BOARD, json!({}));
    std::fs::remove_file(java.join("theme/Mine/board.png")).unwrap();
    std::fs::write(java.join("theme/board.png"), SABAKI_BOARD).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "Mine"}}).to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    std::fs::remove_dir_all(&java).unwrap();
    assert_eq!(committed_board(&mut wb), data_url(SABAKI_BOARD, tmp.path()));
}

#[test]
fn unusable_theme_folders_are_reported_and_leave_the_others_and_existing_themes() {
    use std::os::unix::fs::PermissionsExt;
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "Good", PLAIN_BOARD, json!({}));
    java_theme(&java, "BadColour", SABAKI_BOARD, json!({"pure-board-color": [999, 0, 0], "shadow-size": 0}));
    java_theme(&java, "Locked", SABAKI_BOARD, json!({}));
    std::fs::write(java.join("config.txt"), json!({"ui": {"read-komi": false}}).to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    wb.add_theme("BadColour").unwrap();
    let before = snapshot(&lizzie_core::appearance::themes_dir(wb.storage()).join("BadColour"));
    let locked = java.join("theme/Locked");
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o000)).unwrap();
    let candidate = java_import::read(&JavaSource::new(&java, None));
    std::fs::set_permissions(&locked, std::fs::Permissions::from_mode(0o755)).unwrap();
    let p = wb.preview_java_import(candidate.unwrap());
    let bad = item(&p.items, "file:theme/BadColour");
    assert_eq!((bad.status, bad.reason), (ImportStatus::Incompatible, Some(ImportReason::InvalidValue)));
    assert_eq!(bad.detail.as_deref(), Some("pure-board-color, shadow-size"));
    let locked_item = item(&p.items, "file:theme/Locked");
    assert_eq!((locked_item.status, locked_item.reason), (ImportStatus::Incompatible, Some(ImportReason::Unreadable)));
    assert!(!p.replaces.contains(&ImportTarget::Themes), "an unusable folder replaces nothing");
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert!(lizzie_core::appearance::themes_dir(wb.storage()).join("Good/theme.txt").is_file());
    assert_eq!(snapshot(&lizzie_core::appearance::themes_dir(wb.storage()).join("BadColour")), before);
    assert!(!wb.config().record.read_komi, "the other settings still come over");
}

/// An uncompressed 8×8 TIFF of one colour (Java's `ImageIO` reads TIFF).
fn tiff(rgb: [u8; 3]) -> Vec<u8> {
    let mut out = std::io::Cursor::new(Vec::new());
    image::RgbImage::from_pixel(8, 8, image::Rgb(rgb)).write_to(&mut out, image::ImageFormat::Tiff).unwrap();
    out.into_inner()
}

#[test]
fn a_java_theme_tiff_picture_comes_over_as_png() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    // 棋盘图片 chosen with 所有文件: board.tif; the background is a different board.png.
    java_theme(&java, "TiffTheme", PLAIN_BOARD, json!({"board-image": "board.tif", "background-image": "board.png"}));
    std::fs::write(java.join("theme/TiffTheme/board.tif"), tiff([200, 30, 30])).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "TiffTheme"}}).to_string()).unwrap();
    let java_before = snapshot(&java);
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let default_board = shown_board(&wb, "default");
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "file:theme/TiffTheme").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(wb.view(true).settings.appearance.theme, "TiffTheme");
    let dir = lizzie_core::appearance::themes_dir(wb.storage()).join("TiffTheme");
    let raw: serde_json::Value = serde_json::from_slice(&std::fs::read(dir.join("theme.txt")).unwrap()).unwrap();
    // board.tif becomes board.png; the theme's own board.png keeps its picture under the next free name.
    assert_eq!((&raw["board-image"], &raw["background-image"]), (&json!("board.png"), &json!("board-2.png")));
    assert_eq!(std::fs::read(dir.join("board-2.png")).unwrap(), PLAIN_BOARD);
    let png = std::fs::read(dir.join("board.png")).unwrap();
    let picture = image::load_from_memory_with_format(&png, image::ImageFormat::Png).unwrap().to_rgb8();
    assert_eq!((picture.dimensions(), picture.get_pixel(3, 3).0), ((8, 8), [200, 30, 30]));
    let board = shown_board(&wb, "TiffTheme");
    assert_eq!(board, data_url(&png, tmp.path()));
    assert_ne!(board, default_board);
    assert_eq!(snapshot(&java), java_before, "the Java folder is only read");
}

#[test]
fn a_java_theme_whose_tiff_picture_does_not_decode_is_reported_not_imported() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "Good", PLAIN_BOARD, json!({}));
    java_theme(&java, "BrokenTiff", PLAIN_BOARD, json!({"board-image": "board.tif"}));
    // A TIFF header whose first directory has no entries.
    std::fs::write(java.join("theme/BrokenTiff/board.tif"), b"II*\0\x08\0\0\0\0\0\0\0\0\0").unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "BrokenTiff", "read-komi": false}}).to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    let broken = item(&p.items, "file:theme/BrokenTiff");
    assert_eq!((broken.status, broken.reason), (ImportStatus::Incompatible, Some(ImportReason::Unreadable)));
    assert_eq!(broken.detail.as_deref(), Some("board-image: board.tif"));
    assert_eq!(item(&p.items, "ui.theme").status, ImportStatus::Incompatible);
    assert_eq!(item(&p.items, "file:theme/Good").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let themes = lizzie_core::appearance::themes_dir(wb.storage());
    assert!(!themes.join("BrokenTiff").exists());
    assert!(themes.join("Good/theme.txt").is_file());
    assert_eq!(wb.view(false).settings.appearance.theme, "default");
    assert!(!wb.config().record.read_komi, "the other settings still come over");
}

#[test]
fn a_failed_import_with_ui_pictures_leaves_no_new_theme_folders() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "Mine", PLAIN_BOARD, json!({}));
    std::fs::write(java.join("theme/ui.png"), SABAKI_BOARD).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "Mine", "custom-board-image": "ui.png"}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.dismiss_java_import_offer().unwrap();
    let config_dir = product.join("config");
    let before = snapshot(&config_dir);
    let p = preview(&mut wb, &java);
    let mode = std::fs::metadata(&config_dir).unwrap().permissions();
    let mut read_only = mode.clone();
    std::os::unix::fs::PermissionsExt::set_mode(&mut read_only, 0o555);
    // The themes folder is created first so the import can write into it; the settings cannot.
    std::fs::create_dir(config_dir.join("themes")).unwrap();
    std::fs::set_permissions(&config_dir, read_only).unwrap();
    let failed = wb.apply_java_import(p.id, &[], false, false, true, None);
    std::fs::set_permissions(&config_dir, mode).unwrap();
    assert!(failed.as_ref().is_err_and(|e| e.message.starts_with("saving imported settings failed")), "{failed:?}");
    std::fs::remove_dir(config_dir.join("themes")).unwrap();
    assert_eq!(snapshot(&config_dir), before);
}

#[test]
fn java_theme_replacing_a_user_theme_needs_confirmation_and_failures_leave_themes_as_they_were() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "Mine", SABAKI_BOARD, json!({"shadow-size": 50}));
    std::fs::create_dir_all(java.join("theme/Broken")).unwrap();
    std::fs::write(java.join("theme/Broken/theme.txt"), "{\"shadow-size\": ").unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "Mine"}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.add_theme("Mine").unwrap();
    let before = themes_snapshot(&wb);
    let p = preview(&mut wb, &java);
    assert!(p.replaces.contains(&ImportTarget::Themes));
    assert_eq!(p.replaced_themes, vec!["Mine".to_string()]);
    let broken = item(&p.items, "file:theme/Broken");
    assert_eq!((broken.status, broken.reason), (ImportStatus::Incompatible, Some(ImportReason::Unreadable)));
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(themes_snapshot(&wb), before, "not confirmed: the theme is unchanged");
    // The settings file cannot be written: the theme folders are put back as they were.
    let config_dir = product.join("config");
    let mode = std::fs::metadata(&config_dir).unwrap().permissions();
    let mut read_only = mode.clone();
    std::os::unix::fs::PermissionsExt::set_mode(&mut read_only, 0o555);
    std::fs::set_permissions(&config_dir, read_only).unwrap();
    let failed = wb.apply_java_import(p.id, &[], false, false, true, None);
    std::fs::set_permissions(&config_dir, mode).unwrap();
    assert!(failed.as_ref().is_err_and(|e| e.message.starts_with("saving imported settings failed")), "{failed:?}");
    assert_eq!(themes_snapshot(&wb), before, "a failed import leaves no half-written theme");
    let revision = wb.view(false).settings.theme_files_revision;
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let s = wb.view(false).settings;
    assert_eq!((s.appearance.theme.as_str(), s.theme.shadow_size), ("Mine", 50));
    // The window reloads the images of a theme rewritten in place under the same values.
    assert_ne!(s.theme_files_revision, revision);
    assert_eq!(shown_board(&wb, "Mine"), data_url(SABAKI_BOARD, tmp.path()));
    let names: Vec<String> = lizzie_core::appearance::catalog(wb.storage(), &Default::default()).into_iter().map(|e| e.name).collect();
    assert!(!names.contains(&"Broken".to_string()));
}

#[test]
fn java_theme_named_default_comes_over_renamed_and_stays_selected() {
    // Java's ui.theme "default" reads theme/default/ when that folder exists (Theme.java:63-68).
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    java_theme(&java, "default", PLAIN_BOARD, json!({"shadow-size": 33}));
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "default"}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    let it = item(&p.items, "file:theme/default");
    assert_eq!((it.status, it.value.as_deref()), (ImportStatus::Imported, Some("default (LizzieYzy Next)")));
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let s = wb.view(false).settings;
    assert_eq!((s.appearance.theme.as_str(), s.theme.shadow_size), ("default (LizzieYzy Next)", 33));
    assert_eq!(committed_board(&mut wb), data_url(PLAIN_BOARD, tmp.path()));
    let names: Vec<String> = lizzie_core::appearance::catalog(wb.storage(), &Default::default()).into_iter().map(|e| e.name).collect();
    assert!(names.contains(&"default".to_string()) && names.contains(&"default (LizzieYzy Next)".to_string()), "{names:?}");
    drop(wb);
    let mut reopened = workbench(&product, Recorder::default());
    assert_eq!(reopened.view(false).settings.appearance.theme, "default (LizzieYzy Next)");
    assert_eq!(committed_board(&mut reopened), data_url(PLAIN_BOARD, tmp.path()));
}

#[test]
fn java_default_theme_label_selects_the_default_theme() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {"theme": "默认"}}).to_string()).unwrap();
    let mut wb = workbench(&tmp.path().join("product"), Recorder::default());
    wb.add_theme("Other").unwrap();
    let mut a = wb.view(false).settings.appearance;
    a.theme = "Other".into();
    let theme = wb.view(false).settings.theme;
    wb.set_appearance_settings(a, theme).unwrap();
    let p = preview(&mut wb, &java);
    assert_eq!(item(&p.items, "ui.theme").status, ImportStatus::Imported);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    assert_eq!(wb.view(false).settings.appearance.theme, "default");
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
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
            let row = item(&p.items, &format!("{section}.{key}"));
            assert_eq!(row.status, ImportStatus::Imported, "{section}.{key}");
            if key.starts_with("new-engine-game-") || key.starts_with("pk-") {
                assert_eq!(row.target, Some(ImportTarget::EngineMatch));
            }
        }
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        assert_eq!(wb.config().game.ai_time.mode, if owned { AiTimeMode::EngineOwned }
            else if advanced { AiTimeMode::Advanced } else if kata { AiTimeMode::KataGo }
            else { AiTimeMode::Fixed }, "flags {bits}");
        let before = wb.config().game.clone();
        let autoplay = wb.persisted().autoplay;
        std::fs::write(java.join("config.txt"), json!({"ui": {}}).to_string()).unwrap();
        let p = preview(&mut wb, &java);
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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
        // The hidden 左移/右移 flag is reported as no longer used; -1 is no panel place Java ever writes.
        assert!(p.items.iter().all(|i| i.key != "persist:ui-persist.toolbar-parameter"));
        let row = item(&p.items, "persist:ui-persist.toolbar-parameter[50]");
        assert_eq!((row.status, row.reason), (ImportStatus::NotMigrated, Some(ImportReason::Obsolete)));
        for index in [32, 33, 34] {
            let row = item(&p.items, &format!("persist:ui-persist.toolbar-parameter[{index}]"));
            assert_eq!((row.status, row.target), (ImportStatus::Incompatible, Some(ImportTarget::DetailPanelOrder)));
        }
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
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

#[test]
fn engine_match_imports_dialog_and_toolbar_values_with_one_shared_wrn_owner() {
    // Config.java:1718–1723,1747–1748,1793–1795,1874–1877,1957–1958,1980,2070;
    // DesktopTimeControl.java:102–123; BottomToolbar.java:2437–2520.
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let values = json!({"ui": {
        "engine-pk-black-commands": "first gtp", "engine-pk-black-name": "First",
        "engine-pk-white-commands": "second gtp", "engine-pk-white-name": "Second",
        "new-engine-game-komi": 6.5, "new-engine-game-handicap": 4,
        "engine-game-match-rules": "japanese", "engine-pk-ponder": true,
        "engine-sgf-start": true, "engine-sgf-random": false, "disable-wrn-in-game": false,
        "pk-black-time-mode": "ENGINE_OWNED", "pk-white-time-mode": "RAW_ADVANCED",
        "pk-advance-time-settings": false, "advance-black-time-txt": "time_settings 10 5 1",
        "advance-white-time-txt": "time_settings 20 10 1", "first-engine-min-move": 30,
        "second-engine-min-move": 40, "first-engine-resign-move-counts": 3,
        "second-engine-resign-move-counts": 4, "first-engine-resign-winrate": 2.5,
        "second-engine-resign-winrate": 3.5, "check-random-visits": true,
        "percents-random-visits": 25.0, "show-previous-bestmoves-in-enginegame": true,
        "show-previous-bestmoves-only-first-move": false
    }});
    std::fs::write(java.join("config.txt"), values.to_string()).unwrap();
    let mut toolbar = vec![json!(-1); 51];
    for (index,value) in [(17,50),(18,60),(19,3),(20,500),(21,600),(22,7),(23,1),(24,1),(25,1),(26,1),(27,-1),(36,300),(38,4),(45,12)] {
        toolbar[index] = json!(value);
    }
    for (index,value) in [(30,false),(31,true),(35,false),(37,true),(44,true),(49,true)] { toolbar[index] = json!(value); }
    toolbar[46] = json!(0.75);
    std::fs::write(java.join("persist"),json!({"ui-persist":{"toolbar-parameter":toolbar}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product,Recorder::default());
    let preview = preview(&mut wb,&java);
    for key in values["ui"].as_object().unwrap().keys() {
        let row = item(&preview.items,&format!("ui.{key}"));
        assert_eq!(row.status,ImportStatus::Imported,"{key}");
        assert_eq!(row.target,Some(if key=="disable-wrn-in-game" { ImportTarget::HumanEngineGame } else { ImportTarget::EngineMatch }));
    }
    wb.apply_java_import(preview.id, &[], false, false, true, None).unwrap().unwrap();
    let s = &wb.config().matches;
    assert_eq!((&*s.first.commands,&*s.first.name,&*s.second.commands,&*s.second.name),("first gtp","First","second gtp","Second"));
    assert_eq!((s.komi,s.handicap,s.rules.as_str()),(6.5,4,"japanese"));
    assert!(s.ponder && s.sgf_start && !s.sgf_random && !s.advanced_time);
    assert_eq!((s.first_time_mode,s.second_time_mode),(lizzie_core::settings::MatchTimeMode::EngineOwned,lizzie_core::settings::MatchTimeMode::RawAdvanced));
    assert_eq!((s.first_time_command.as_str(),s.second_time_command.as_str()),("time_settings 10 5 1","time_settings 20 10 1"));
    assert_eq!((s.first_resign.min_move,s.first_resign.consecutive_moves,s.first_resign.winrate),(30,3,2.5));
    assert_eq!((s.second_resign.min_move,s.second_resign.consecutive_moves,s.second_resign.winrate),(40,4,3.5));
    assert!(s.check_random_visits && s.show_previous_bestmoves && !s.previous_bestmove_only);
    assert_eq!(s.random_visits_percent,25.0);
    assert!(!wb.config().game.disable_wrn);
    let v = wb.persisted().matches.clone();
    assert_eq!((v.first_move_visits,v.second_move_visits,v.first_visits,v.second_visits),(50,60,500,600));
    assert_eq!((v.first_seconds,v.second_seconds,v.batch_limit),(3.0,4.0,7));
    assert!(v.batch_enabled && v.continue_position && v.first_move_visits_enabled && v.visits_enabled && v.time_enabled);
    assert_eq!(v.kind,lizzie_core::api::GameKind::Genmove);
    assert!(!v.auto_save && !v.exchange_colors && v.max_moves_enabled && v.random_moves_enabled && v.save_winrate);
    assert_eq!((v.max_moves,v.random_moves,v.random_winrate_diff),(300,12,0.75));
    drop(wb);
    assert_eq!(workbench(&product,Recorder::default()).persisted().matches,v);
}

#[test]
fn humansl_training_and_model_import_preserve_java_files_and_migrate_live_correction() {
    use lizzie_core::settings::{OpponentPreset, PlayerColor, TrainingMode};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    let model = java.join("human-sl-models/custom.bin.gz");
    std::fs::create_dir_all(model.parent().unwrap()).unwrap();
    std::fs::File::create(&model).unwrap().set_len(1024 * 1024 + 1).unwrap();
    let source = json!({"ui": {
        "katago-human-sl-model-path": "human-sl-models/custom.bin.gz",
        "human-sl-training-last-settings": {
            "mode": "LIVE_CORRECTION", "opponent-preset": "ONLINE_9D", "dan-rank": false,
            "rank": 18, "player-color": "WHITE", "move-time-seconds": 86400,
            "handicap": 2, "komi": 0.5, "advanced-visible": true
        }
    }}).to_string();
    std::fs::write(java.join("config.txt"), &source).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for target in [ImportTarget::HumanSlTraining, ImportTarget::HumanSlModel] {
        assert!(p.items.iter().any(|row| row.target == Some(target) && row.status == ImportStatus::Imported));
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let h = &wb.config().human_sl;
    assert_eq!(h.model_path.as_deref().map(Path::new), Some(model.as_path()));
    let s = &h.training;
    assert_eq!((s.mode, s.opponent, s.player_color), (TrainingMode::LiveAnalysis, OpponentPreset::Online9d, PlayerColor::White));
    assert_eq!((s.rank, s.move_time_seconds, s.handicap, s.komi), (18, 86400, 2, 0.5));
    assert!(!s.dan_rank && h.advanced_visible);
    assert_eq!(wb.view(false).settings.human_sl.model.status, lizzie_core::api::HumanSlModelStatus::Installed);
    assert_eq!(saved_config(&product).human_sl, wb.config().human_sl);
    assert_eq!(std::fs::read_to_string(java.join("config.txt")).unwrap(), source);
    assert_eq!(std::fs::metadata(&model).unwrap().len(), 1024 * 1024 + 1);
}

#[test]
fn humansl_java_corrupt_or_future_fields_fall_back_independently() {
    use lizzie_core::settings::{OpponentPreset, PlayerColor, TrainingMode};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({"ui": {
        "human-sl-training-last-settings": {
            "mode": "FUTURE_MODE", "opponent-preset": "UNKNOWN_STYLE", "dan-rank": true,
            "rank": 99, "player-color": "PURPLE", "move-time-seconds": 17,
            "handicap": 99, "komi": "not-a-number", "advanced-visible": true
        }
    }}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let s = &wb.config().human_sl.training;
    assert_eq!((s.mode, s.opponent, s.player_color), (TrainingMode::PostGameReview, OpponentPreset::Rank, PlayerColor::Random));
    assert_eq!((s.rank, s.move_time_seconds, s.handicap, s.komi), (9, 10, 9, 7.5));
    assert!(wb.config().human_sl.advanced_visible);
}

#[test]
fn tsumego_choices_capture_thresholds_and_button_import_like_java_reads_them() {
    use lizzie_core::settings::{CaptureSettings, TsumegoFrameSettings};
    use lizzie_core::tsumego::{TsumegoAttacker, TsumegoKoThreat, TsumegoToPlay};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    // An attacker outside the dialog's choices selects no radio button and builds like 自动检测;
    // the gray offset lives under `capture-gray-percent`.
    let source = json!({"ui": {
        "tsume-go-to-play": 3, "tsume-go-attaker": 7, "tsume-go-ko-threat": 4, "tsume-go-wall-distance": 5,
        "show-tsume-go-menu": false,
        "capture-black-offset": 100, "capture-black-percent": 30, "capture-white-offset": 90,
        "capture-white-percent": 12, "capture-gray-percent": 33
    }}).to_string();
    std::fs::write(java.join("config.txt"), &source).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for target in [ImportTarget::Tsumego, ImportTarget::CaptureTsumego] {
        assert!(p.items.iter().any(|row| row.target == Some(target) && row.status == ImportStatus::Imported));
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let t = &wb.config().tsumego;
    assert_eq!(
        t.frame,
        TsumegoFrameSettings {
            to_play: TsumegoToPlay::White,
            attacker: TsumegoAttacker::Auto,
            ko_threat: TsumegoKoThreat::None,
            wall_distance: 5,
        }
    );
    assert!(!t.show_button);
    assert_eq!(
        t.capture,
        CaptureSettings { black_offset: 100, black_percent: 30, white_offset: 90, white_percent: 12, gray_offset: 33 }
    );
    assert_eq!(saved_config(&product).tsumego, *t);
    assert_eq!(std::fs::read_to_string(java.join("config.txt")).unwrap(), source);
}

#[test]
fn top_toolbar_groups_import_by_value() {
    use lizzie_core::settings::TopToolbarSettings;
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    // Every value but show-playout-control-in-menu differs from its default.
    let source = json!({"ui": {
        "show-double-menu-btn": true, "show-basic-btn": false, "show-edit-bar": false,
        "show-force-menu": false, "show-double-menu-game-control": false,
        "show-time-control-in-menu": true, "show-playout-control-in-menu": false,
        "show-analyze-controller": true, "show-double-menu-var": false, "show-double-menu-moveinfo": true,
        "show-rule-menu": false, "show-param-menu": false, "show-goban-menu": false,
        "show-saveload-menu": false, "auto-wrap-tool-bar": false
    }})
    .to_string();
    std::fs::write(java.join("config.txt"), &source).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in [
        "ui.show-double-menu-btn", "ui.show-basic-btn", "ui.show-edit-bar", "ui.show-force-menu",
        "ui.show-double-menu-game-control", "ui.show-time-control-in-menu", "ui.show-playout-control-in-menu",
        "ui.show-analyze-controller", "ui.show-double-menu-var", "ui.show-double-menu-moveinfo", "ui.show-rule-menu",
        "ui.show-param-menu", "ui.show-goban-menu", "ui.show-saveload-menu", "ui.auto-wrap-tool-bar",
    ] {
        let row = p.items.iter().find(|row| row.key == key).unwrap();
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::TopToolbar)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let expected = TopToolbarSettings {
        show_double_menu_button: true,
        show_basic_buttons: false,
        show_edit_bar: false,
        show_force: false,
        show_game_controls: false,
        show_time_limit: true,
        show_visit_limit: false,
        show_analyze_sides: true,
        show_candidate_sides: false,
        show_candidate_info: true,
        show_rules: false,
        show_parameters: false,
        show_board_size: false,
        show_save_load: false,
        auto_wrap: false,
    };
    assert_eq!(wb.config().top_toolbar, expected);
    assert_eq!(saved_config(&product).top_toolbar, expected);
}

#[test]
fn bottom_toolbar_buttons_and_detail_panel_order_import_by_value() {
    use lizzie_core::settings::{BottomButtonSettings, DetailPanelOrder};
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    // Config.java:1909-1936: every value differs from its default (badMoves has none of its own).
    let source = json!({"ui": {
        "liveButton": false, "kataEstimate": false, "batchOpen": false, "openfile": true,
        "savefile": true, "finalScore": true, "heatMap": false, "badMoves": true,
        "analyze-list": true, "refresh": true, "analyse": false, "tryPlay": true, "setMain": false,
        "backMain": true, "clearButton": false, "deleteMove": false, "move-rank": true, "move": false,
        "coords": false, "autoPlay": false, "show-detailed-toolbar-menu": true
    }})
    .to_string();
    std::fs::write(java.join("config.txt"), &source).unwrap();
    let mut toolbar = vec![json!(-1); 51];
    toolbar[32] = json!(2); toolbar[33] = json!(0); toolbar[34] = json!(1);
    std::fs::write(java.join("persist"), json!({"ui-persist": {"toolbar-parameter": toolbar}}).to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in [
        "ui.liveButton", "ui.kataEstimate", "ui.batchOpen", "ui.openfile", "ui.savefile", "ui.finalScore",
        "ui.heatMap", "ui.badMoves", "ui.analyze-list", "ui.refresh", "ui.analyse", "ui.tryPlay", "ui.setMain",
        "ui.backMain", "ui.clearButton", "ui.deleteMove", "ui.move-rank", "ui.move", "ui.coords", "ui.autoPlay",
        "ui.show-detailed-toolbar-menu",
    ] {
        let row = p.items.iter().find(|row| row.key == key).unwrap();
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::BottomButtons)), "{key}");
    }
    for index in [32, 33, 34] {
        let row = item(&p.items, &format!("persist:ui-persist.toolbar-parameter[{index}]"));
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::DetailPanelOrder)));
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let expected = BottomButtonSettings {
        live: false, kata_estimate: false, batch_open: false, open_file: true, save_file: true,
        final_score: true, heat_map: false, bad_moves: Some(true), analyze_list: true, refresh: true,
        analyse: false, try_play: true, set_main: false, back_main: true, clear_board: false,
        delete_move: false, move_rank: true, move_numbers: false, coords: false, auto_play: false,
        detail_button: true,
    };
    let order = DetailPanelOrder { auto_analysis: 2, engine_match: 0, auto_play: 1 };
    assert_eq!(wb.config().bottom_buttons, expected);
    assert_eq!(saved_config(&product).bottom_buttons, expected);
    assert_eq!(wb.persisted().detail_panel_order, order);
    drop(wb);
    assert_eq!(workbench(&product, Recorder::default()).persisted().detail_panel_order, order, "persisted on import");
}

#[test]
fn move_rect_and_click_review_import_by_value() {
    use lizzie_core::settings::MoveRect;
    // Config.showrect: 0 始终, 1 仅对局时, any other number 否 (ConfigDialog2.java:1215-1221).
    for (rect, expected) in [(0, MoveRect::Always), (2, MoveRect::Never), (7, MoveRect::Never)] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        let source = json!({"ui": {"show-move-rect": rect, "enable-click-review": true}}).to_string();
        std::fs::write(java.join("config.txt"), source).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        for (key, target) in [("ui.show-move-rect", ImportTarget::MoveRect), ("ui.enable-click-review", ImportTarget::ClickReview)] {
            let row = item(&p.items, key);
            assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(target)), "{key}");
        }
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        let edit = &saved_config(&product).edit;
        assert_eq!((edit.move_rect, edit.click_review), (expected, true), "show-move-rect {rect}");
    }
}

#[test]
fn painting_color_imports_by_value() {
    use lizzie_core::settings::PaintingColor;
    // DrawPainting opens with 1 green, 2 red, anything else blue (DrawPainting.java:75-85).
    for (value, expected) in [(1, PaintingColor::Green), (2, PaintingColor::Red), (5, PaintingColor::Blue)] {
        let tmp = tempfile::tempdir().unwrap();
        let java = tmp.path().join("java");
        std::fs::create_dir_all(&java).unwrap();
        std::fs::write(java.join("config.txt"), json!({"ui": {"last-painting-color": value}}).to_string()).unwrap();
        let product = tmp.path().join("product");
        let mut wb = workbench(&product, Recorder::default());
        let p = preview(&mut wb, &java);
        let row = item(&p.items, "ui.last-painting-color");
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::PaintingColor)), "{value}");
        wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
        assert_eq!(saved_config(&product).edit.painting_color, expected, "last-painting-color {value}");
    }
}

#[test]
fn variation_tree_width_settings_import_by_value() {
    let tmp = tempfile::tempdir().unwrap();
    let java = tmp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    let source = json!({"ui": {"show-scroll-variation": false, "max-tree-width": 600, "ignore-out-of-width": true}});
    std::fs::write(java.join("config.txt"), source.to_string()).unwrap();
    let product = tmp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    let p = preview(&mut wb, &java);
    for key in ["ui.show-scroll-variation", "ui.max-tree-width", "ui.ignore-out-of-width"] {
        let row = item(&p.items, key);
        assert_eq!((row.status, row.target), (ImportStatus::Imported, Some(ImportTarget::VariationTree)), "{key}");
    }
    wb.apply_java_import(p.id, &[], false, false, true, None).unwrap().unwrap();
    let d = &saved_config(&product).display;
    assert_eq!((d.scroll_variation, d.max_tree_width, d.ignore_out_of_width), (false, 600, true));
}

#[test]
fn teacher_settings_preserve_legacy_defaults_and_java_clamping() {
    use lizzie_core::api::CommentaryProvider;
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    let teacher = "# Java pre-provider settings\r\nbaseUrl=   \r\nmodel=example\\u002dmodel\r\nteacher.rankMode=d\r\nteacher.rankNum=99\r\nteacher.styleIndex=-8\r\nteacher.densityIndex=9\r\nteacher.paceIndex=invalid\r\nteacher.variationIndex=-1\r\n";
    std::fs::write(java.join("teacher.properties"), teacher).unwrap();
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let preview = preview(&mut wb, &java);
    let item = preview.items.iter().find(|item| item.key == "file:teacher.properties").unwrap();
    assert_eq!((item.status, item.target, item.credential), (ImportStatus::Imported, Some(ImportTarget::Commentary), false));
    assert!(preview.commentary_credential.is_none());
    wb.apply_java_import(preview.id, &[], false, false, false, None).unwrap().unwrap();
    let settings = &wb.config().commentary;
    assert_eq!(settings.provider, CommentaryProvider::ApiKey);
    assert_eq!(settings.base_url, "https://api.openai.com/v1");
    assert_eq!(settings.model, "example-model");
    assert_eq!((settings.preferences.rank_mode.as_str(), settings.preferences.rank_num), ("d", 9));
    assert_eq!((settings.preferences.style_index, settings.preferences.density_index, settings.preferences.pace_index, settings.preferences.variation_index), (0, 2, 1, 0));
    assert_eq!(std::fs::read_to_string(java.join("teacher.properties")).unwrap(), teacher);
}

#[test]
fn explicit_provider_does_not_inherit_the_legacy_openai_address() {
    use lizzie_core::api::CommentaryProvider;
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    std::fs::write(java.join("teacher.properties"), "provider=CHATGPT\nbaseUrl=\nmodel=\nteacher.rankMode=invalid\nteacher.rankNum=not-an-int\n").unwrap();
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let settings = candidate.mapped.commentary.as_ref().unwrap();
    assert_eq!(settings.provider, CommentaryProvider::ChatGpt);
    assert!(settings.base_url.is_empty() && settings.model.is_empty());
    assert_eq!((settings.preferences.rank_mode.as_str(), settings.preferences.rank_num), ("k", 5));
    let session = candidate.items.iter().find(|item| item.key == "credential:chatgpt-session").unwrap();
    assert_eq!((session.status, session.reason), (ImportStatus::NotMigrated, Some(ImportReason::Credential)));
}

#[test]
fn legacy_commentary_key_is_a_separate_unconfirmed_choice_and_never_enters_core_state() {
    const KEY: &str = "private-legacy-commentary-key";
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    let teacher = format!("model=example\nrememberApiKey=true\napiKey={KEY}\n");
    std::fs::write(java.join("teacher.properties"), &teacher).unwrap();
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let preview = preview(&mut wb, &java);
    let choice = preview.commentary_credential.as_ref().unwrap();
    assert_eq!(choice.source, "teacher.properties");
    assert_eq!(choice.base_url, "https://api.openai.com/v1");
    assert!(wb.java_commentary_import(preview.id + 1).is_none());
    assert!(!serde_json::to_string(&preview).unwrap().contains(KEY));
    let report = wb.apply_java_import(preview.id, &[], false, false, false, None).unwrap().unwrap();
    let item = report.items.iter().find(|item| item.key == "credential:commentary-api-key").unwrap();
    assert_eq!((item.status, item.reason), (ImportStatus::NotMigrated, Some(ImportReason::CredentialNotConfirmed)));
    assert!(!wb.config().commentary.remember_api_key);
    assert!(wb.java_commentary_import(preview.id).is_none());
    for text in [serde_json::to_string(&report).unwrap(), serde_json::to_string(wb.config()).unwrap(), serde_json::to_string(&wb.inspect()).unwrap()] {
        assert!(!text.contains(KEY));
    }
    assert_eq!(std::fs::read_to_string(java.join("teacher.properties")).unwrap(), teacher);
    let command: lizzie_core::api::Command = serde_json::from_value(json!({"type":"ApplyJavaImport", "preview":1, "copy_credentials":[], "replace":false, "recovery":null})).unwrap();
    assert!(matches!(command, lizzie_core::api::Command::ApplyJavaImport { copy_commentary_key: false, .. }));
}

#[test]
fn remembered_commentary_key_selection_does_not_share_engine_credential_consent() {
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    std::fs::write(java.join("teacher.properties"), "baseUrl=https://provider.example/v1\nmodel=example\nrememberApiKey=true\n").unwrap();
    let mut candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    assert!(candidate.commentary_credential.is_none());
    // The platform adapter supplies only the fact that a readable secure-store key exists.
    candidate.set_commentary_credential("secure-store");
    let items = candidate.items_after(&candidate.credential_engines(), true, true);
    let credential = items.iter().find(|item| item.key == "credential:commentary-api-key").unwrap();
    assert_eq!(credential.reason, Some(ImportReason::CredentialNotConfirmed));
    assert_eq!(candidate.commentary_credential.unwrap().source, "secure-store");
}

#[test]
fn java_remembering_its_key_in_the_credential_store_is_told_to_the_adapter() {
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    // TeacherSettings.load: Boolean.parseBoolean, and a legacy plain-text key turns remembering off.
    for (teacher, remembered) in [
        ("baseUrl=https://provider.example/v1\nrememberApiKey=TRUE\n", true),
        ("baseUrl=https://provider.example/v1\nrememberApiKey=false\n", false),
        ("baseUrl=https://provider.example/v1\n", false),
        ("baseUrl=https://provider.example/v1\nrememberApiKey=true\napiKey=legacy\n", false),
    ] {
        std::fs::write(java.join("teacher.properties"), teacher).unwrap();
        let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
        assert_eq!(candidate.commentary_key_remembered, remembered, "{teacher:?}");
        // Remembering here still waits for the confirmed copy.
        assert!(!candidate.mapped.commentary.as_ref().unwrap().remember_api_key);
    }
}

#[test]
fn imported_commentary_settings_remember_nothing_until_a_key_is_copied() {
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), "{}").unwrap();
    std::fs::write(java.join("teacher.properties"), "provider=API_KEY\nbaseUrl=https://provider.example/v1\nmodel=example\nrememberApiKey=true\n").unwrap();
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let preview = preview(&mut wb, &java);
    wb.apply_java_import(preview.id, &[], false, false, false, None).unwrap().unwrap();
    let settings = &wb.config().commentary;
    assert_eq!((settings.base_url.as_str(), settings.model.as_str()), ("https://provider.example/v1", "example"));
    // Only a confirmed, successful copy into this client's credential store turns remembering on.
    assert!(!settings.remember_api_key);
}

fn cloud_java(root: &Path, remote: serde_json::Value) -> PathBuf {
    let java = root.join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("local-engine"), "fixture").unwrap();
    std::fs::write(java.join("config.txt"), json!({
        "ui": { "autoload-empty": true, "last-engine": 2, "default-engine": 2 },
        "leelaz": {
            "remote-compute": remote,
            "engine-settings-list": [
                { "id": "local", "name": "Local", "command": "./local-engine gtp" },
                { "id": "zhizi", "name": "Cloud", "command": "remote-compute://zhizi", "useJavaSSH": true },
                { "id": "custom", "name": "Custom", "command": "remote-compute://custom-websocket", "isDefault": true },
            ],
        },
    }).to_string()).unwrap();
    java
}

#[test]
fn cloud_markers_map_before_local_or_ssh_launch_and_keep_engine_selection() {
    use lizzie_core::cloud::{DiscoverySource, Provider};
    use lizzie_core::engine::cloud::CloudProvider;
    let temp = tempfile::tempdir().unwrap();
    let java = cloud_java(temp.path(), json!({
        "provider": "custom", "zhizi-identifier": "account@example.invalid",
        "zhizi-args": "--gpu-type 3x --kata-weight 10b512t",
        "custom-remote-code": "复制 ＷＳＳ：／／example.invalid／analysis 更多文字",
        "zhizi-catalog-updated-at": 1234,
        "zhizi-engine-catalog": {
            "serverVersion": "java-cache", "defaultKataWeight": "10b512t",
            "supportKataWeights": [{ "name": "test-weight", "description": "saved weight", "source": "CACHED_LEGACY" }],
        },
    }));
    let before = snapshot(&java);
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let p = preview(&mut wb, &java);
    assert!(p.engines.iter().all(|engine| engine.problems.is_empty()));
    assert!(p.engines.iter().all(|engine| !engine.credential));
    wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    let config = wb.config();
    assert_eq!(config.cloud.provider, Provider::Custom);
    assert_eq!(config.cloud.custom_url, "WSS://example.invalid/analysis");
    assert_eq!(config.cloud.catalog_updated_at, 1234);
    assert!(config.cloud.catalog.weights.iter().any(|weight| weight.name == "test-weight" && weight.source == DiscoverySource::CachedLegacy));
    assert_eq!(config.engine.last_engine.as_deref(), Some("custom"));
    assert!(config.engine.engines[2].is_default);
    assert!(config.engine.engines[0].cloud.is_none());
    let cloud = &config.engine.engines[1];
    assert!(cloud.ssh.is_none());
    assert_eq!(cloud.command, "remote-compute://zhizi");
    assert_eq!(cloud.cloud.as_ref().unwrap().provider, CloudProvider::Zhizi);
    assert_eq!(cloud.cloud.as_ref().unwrap().args, config.cloud.args);
    assert_eq!(config.engine.engines[2].cloud.as_ref().unwrap().url, config.cloud.custom_url);
    assert_eq!(snapshot(&java), before);
}

#[test]
fn cloud_secrets_have_independent_availability_and_never_enter_preview_or_core_state() {
    const TOKEN: &str = "CLOUD-TOKEN-SECRET";
    const PASSWORD: &str = "päss";
    const ENCODED: &str = "cMOkc3M=";
    let temp = tempfile::tempdir().unwrap();
    let java = cloud_java(temp.path(), json!({
        "provider": "zhizi", "zhizi-identifier": "user@example.invalid",
        "zhizi-account-token": TOKEN, "zhizi-password-v1": ENCODED,
        "remember-zhizi-token": true, "remember-zhizi-password": true,
    }));
    let before = snapshot(&java);
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let debug = format!("{candidate:?}");
    let choices = candidate.cloud_credentials.as_ref().unwrap();
    assert!(choices.token && choices.password);
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let p = wb.preview_java_import(candidate);
    assert!(wb.java_cloud_import(p.id + 1).is_none());
    let (_, settings, availability) = wb.java_cloud_import(p.id).unwrap();
    assert_eq!(availability.identifier, settings.identifier);
    // These source reads correspond to independent shell checkboxes, not per-engine consent.
    assert_eq!(java_import::read_java_cloud_secret(&java, false).as_deref(), Some(TOKEN));
    assert_eq!(java_import::read_java_cloud_secret(&java, true).as_deref(), Some(PASSWORD));
    let report = wb.apply_java_import(p.id, &[0, 1, 2], false, false, false, None).unwrap().unwrap();
    assert!(!wb.config().cloud.remember_token && !wb.config().cloud.remember_password);
    for key in ["credential:cloud-token", "credential:cloud-password"] {
        assert_eq!(item(&report.items, key).reason, Some(ImportReason::CredentialNotConfirmed));
    }
    for text in [debug, serde_json::to_string(&p).unwrap(), serde_json::to_string(&report).unwrap(), serde_json::to_string(wb.config()).unwrap()] {
        for secret in [TOKEN, PASSWORD, ENCODED] { assert!(!text.contains(secret)); }
    }
    assert!(wb.java_cloud_import(p.id).is_none());
    assert_eq!(snapshot(&java), before);
}

#[test]
fn custom_cloud_url_requires_engine_consent_including_top_level_settings() {
    let temp = tempfile::tempdir().unwrap();
    let url = "wss://example.invalid/analysis?accessCode=CUSTOM-URL-SECRET";
    let java = cloud_java(temp.path(), json!({ "provider": "custom", "custom-remote-code": url }));
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    assert!(!format!("{candidate:?}").contains("CUSTOM-URL-SECRET"));
    assert_eq!(candidate.credential_engines(), vec![2]);
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let p = wb.preview_java_import(candidate);
    assert!(!serde_json::to_string(&p).unwrap().contains("CUSTOM-URL-SECRET"));
    let report = wb.apply_java_import(p.id, &[1], false, false, false, None).unwrap().unwrap();
    assert_eq!(wb.config().engine.engines.len(), 2);
    assert!(wb.config().cloud.custom_url.is_empty());
    assert!(wb.config().engine.last_engine.is_none());
    assert_eq!(item(&report.items, "leelaz.remote-compute.custom-remote-code").reason, Some(ImportReason::CredentialNotConfirmed));
    assert!(!serde_json::to_string(&report).unwrap().contains("CUSTOM-URL-SECRET"));
    let p = preview(&mut wb, &java);
    let report = wb.apply_java_import(p.id, &[2], false, false, true, None).unwrap().unwrap();
    assert_eq!(wb.config().cloud.custom_url, url);
    assert_eq!(wb.config().engine.engines[2].cloud.as_ref().unwrap().url, url);
    assert_eq!(wb.config().engine.last_engine.as_deref(), Some("custom"));
    assert!(!serde_json::to_string(&report).unwrap().contains("CUSTOM-URL-SECRET"));
}

#[test]
fn native_cloud_availability_does_not_select_the_other_secret_and_defaults_are_java_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let java = cloud_java(temp.path(), json!({ "zhizi-identifier": "user@example.invalid", "zhizi-catalog-updated-at": -9 }));
    let mut candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    candidate.set_cloud_credential_available(true);
    candidate.set_cloud_credential_available(true);
    assert_eq!(candidate.items.iter().filter(|item| item.key == "credential:cloud-password").count(), 1);
    assert!(!candidate.cloud_credentials.as_ref().unwrap().token);
    assert!(candidate.cloud_credentials.as_ref().unwrap().password);
    let (config, _) = candidate.mapped.apply(&Config::default(), &Persisted::default(), &[], false, false);
    assert_eq!(config.cloud.provider, lizzie_core::cloud::Provider::Local);
    assert_eq!(config.cloud.args, lizzie_core::cloud::VIP_ARGS);
    assert_eq!(config.cloud.catalog_updated_at, 0);
    assert_eq!(config.cloud.catalog, lizzie_core::cloud::Catalog::default());
    std::fs::write(java.join("config.txt"), json!({"leelaz":{"remote-compute":{"zhizi-password-v1":"%%%invalid"}}}).to_string()).unwrap();
    assert!(java_import::read_java_cloud_secret(&java, true).is_none());
}

#[test]
fn rejected_cloud_replacement_and_failed_transaction_leave_java_and_current_state_unchanged() {
    let temp = tempfile::tempdir().unwrap();
    let java = cloud_java(temp.path(), json!({ "provider": "zhizi", "zhizi-identifier": "new-account" }));
    std::fs::write(java.join("persist"), json!({"filesystem":{"last-folder":"/imported-games"}}).to_string()).unwrap();
    let before_java = snapshot(&java);
    let product = temp.path().join("product");
    let mut wb = workbench(&product, Recorder::default());
    wb.set_play_sound(false).unwrap();
    let before = wb.config().clone();
    let before_disk = std::fs::read(storage(&product).config_file).unwrap();
    let p = preview(&mut wb, &java);
    assert!(p.replaces.contains(&ImportTarget::Cloud));
    assert_eq!(wb.apply_java_import(p.id, &[], false, false, false, None).unwrap(), Err(IMPORT_NEEDS_REPLACE));
    assert_eq!(wb.config(), &before);
    std::fs::create_dir_all(storage(&product).persist_file.join("blocker")).unwrap();
    assert!(wb.apply_java_import(p.id, &[], false, false, true, None).is_err());
    assert_eq!(wb.config(), &before);
    assert_eq!(std::fs::read(storage(&product).config_file).unwrap(), before_disk);
    assert!(wb.java_cloud_import(p.id).is_some());
    assert_eq!(snapshot(&java), before_java);
}

#[test]
fn remembered_cloud_choices_are_metadata_only_and_shell_staging_is_preview_scoped() {
    let temp = tempfile::tempdir().unwrap();
    let java = cloud_java(temp.path(), json!({
        "zhizi-identifier": "native-account", "remember-zhizi-token": true, "remember-zhizi-password": false,
    }));
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    let availability = candidate.cloud_credentials.as_ref().unwrap();
    assert!(availability.token && !availability.password);
    assert!(java_import::read_java_cloud_secret(&java, false).is_none());
    let mut wb = workbench(&temp.path().join("product"), Recorder::default());
    let p = wb.preview_java_import(candidate);
    assert!(!wb.stage_java_cloud_preferences(p.id + 1, true, true));
    assert!(!wb.config().cloud.remember_token);
    assert!(wb.stage_java_cloud_preferences(p.id, true, false));
    assert!(!wb.config().cloud.remember_token, "staging never changes live settings");
    wb.apply_java_import(p.id, &[], false, false, false, None).unwrap().unwrap();
    assert!(wb.config().cloud.remember_token && !wb.config().cloud.remember_password);
    assert!(!wb.stage_java_cloud_preferences(p.id, false, false));
}

#[test]
fn cloud_marker_without_remote_settings_uses_fixed_java_defaults() {
    let temp = tempfile::tempdir().unwrap();
    let java = temp.path().join("java");
    std::fs::create_dir_all(&java).unwrap();
    std::fs::write(java.join("config.txt"), json!({
        "ui": { "autoload-last": true, "last-engine": 0 },
        "leelaz": { "engine-settings-list": [{"id":"cloud-only","command":"remote-compute://zhizi","isDefault":true}] },
    }).to_string()).unwrap();
    let candidate = java_import::read(&JavaSource::new(&java, None)).unwrap();
    assert!(candidate.engines[0].problems.is_empty());
    assert!(!candidate.cloud_credentials.as_ref().unwrap().token);
    let (config, _) = candidate.mapped.apply(&Config::default(), &Persisted::default(), &[], false, false);
    assert_eq!(config.cloud, lizzie_core::cloud::CloudSettings::default());
    assert_eq!(config.engine.startup, EngineStartup::Last);
    assert_eq!(config.engine.last_engine.as_deref(), Some("cloud-only"));
    assert!(config.engine.engines[0].is_default);
    assert_eq!(config.engine.engines[0].cloud.as_ref().unwrap().args, lizzie_core::cloud::VIP_ARGS);
}
