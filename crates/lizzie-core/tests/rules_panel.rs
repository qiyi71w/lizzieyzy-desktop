//! 规则面板 确定 (Java `RulesQuickSwitch.apply` and `SetKataRules.applySelectedRules`) through the
//! public `Workbench` operations, with a controlled KataGo that reads back the rules it was set:
//! the komi goes to the engine before the rules, a handicap game keeps its komi, 启动时自动应用
//! makes a rule set the default of new games until the startup rules change, a failed save reaches
//! no engine, and the panel's recognition of the chosen rule set.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};

use lizzie_core::api::{ApiError, CommandResult, EditCommand, OpenSource, RulesStatus};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::rules::RuleSet;
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::sgf;
use lizzie_core::record::RecordFormat;
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};

#[path = "support/permissions.rs"]
mod permissions;

/// Each engine: the commands it was sent and not yet answered, the rules it holds, whether it
/// refuses `kata-set-rules`, and a `komi` command it refuses.
#[derive(Default)]
struct Engine {
    sent: Vec<String>,
    rules: String,
    refuse_rules: bool,
    refuse_komi: Option<String>,
}

#[derive(Clone, Default)]
struct World(Arc<Mutex<HashMap<InstanceId, Engine>>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().get_mut(&id).unwrap().sent)
    }
    fn last(&self) -> InstanceId {
        *self.0.lock().unwrap().keys().max().unwrap()
    }
    fn refuse_rules(&self, id: InstanceId) {
        self.0.lock().unwrap().get_mut(&id).unwrap().refuse_rules = true;
    }
    fn accept_rules(&self, id: InstanceId) {
        self.0.lock().unwrap().get_mut(&id).unwrap().refuse_rules = false;
    }
    fn refuse_komi(&self, id: InstanceId, command: &str) {
        self.0.lock().unwrap().get_mut(&id).unwrap().refuse_komi = Some(command.into());
    }
}

struct FakeIo(InstanceId, World);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        (self.1).0.lock().unwrap().get_mut(&self.0).unwrap().sent.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, _spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        let engine = Engine { rules: JAPANESE.into(), ..Engine::default() };
        self.0 .0.lock().unwrap().insert(id, engine);
        Ok(Box::new(FakeIo(id, self.0.clone())))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";
/// KataGo's `kata-get-rules` answers; the engine starts with Japanese rules.
const JAPANESE: &str = r#"{"friendlyPassOk":false,"hasButton":false,"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"SEKI","whiteHandicapBonus":"0"}"#;
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command as KataGo does: `kata-set-rules` takes the rules (unless the
/// engine refuses them), `kata-get-rules` reads back the rules held. Returns the commands answered,
/// without their ids, in the order they were sent.
fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId) -> Vec<String> {
    let mut answered = Vec::new();
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return answered;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = {
                let mut engines = world.0.lock().unwrap();
                let engine = engines.get_mut(&id).unwrap();
                match cmd.split(' ').next().unwrap() {
                    "name" => Ok(Some("KataGo".to_string())),
                    "version" => Ok(Some("1.18.1".to_string())),
                    "list_commands" => Ok(Some(KATA.to_string())),
                    "kata-set-rules" if engine.refuse_rules => Err(()),
                    "komi" if engine.refuse_komi.as_deref() == Some(cmd) => Err(()),
                    "kata-set-rules" => {
                        engine.rules = cmd["kata-set-rules ".len()..].to_string();
                        Ok(Some(String::new()))
                    }
                    "kata-get-rules" => Ok(Some(engine.rules.clone())),
                    "kata-analyze" => Ok(None),
                    _ => Ok(Some(String::new())),
                }
            };
            match reply {
                Ok(Some(text)) => {
                    out(wb, id, &format!("={n} {text}"));
                    out(wb, id, "");
                }
                Ok(None) => out(wb, id, &format!("={n}")),
                Err(()) => {
                    out(wb, id, &format!("?{n} illegal rules"));
                    out(wb, id, "");
                }
            }
            answered.push(cmd.to_string());
        }
    }
}

fn storage(dir: &std::path::Path) -> Storage {
    Storage::in_dirs(&dir.join("config"), &dir.join("data"))
}

/// The workbench in `dir`, its KataGo started and answered; a first start saves the engine.
fn started(dir: &std::path::Path, world: &World) -> (Workbench, InstanceId) {
    let mut wb = Workbench::new(storage(dir), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    if wb.config().engine.engines.is_empty() {
        wb.set_auto_quick_analyze(false).unwrap();
        let profile = EngineProfile { name: "KataGo".into(), command: "/opt/katago gtp".into(), ..EngineProfile::default() };
        wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    }
    wb.start_engines();
    let id = world.last();
    answer_all(&mut wb, world, id);
    (wb, id)
}

/// The rules of rule set `id` as the panel sends them.
fn rules_of(id: &str) -> String {
    RuleSet::by_id(id).unwrap().rules().to_gtp_argument()
}

fn caption(wb: &mut Workbench) -> Option<String> {
    wb.view(false).engine.rules_key
}

fn key(k: &str) -> Option<String> {
    Some(format!("LizzieFrame.currentRules.{k}"))
}

fn status(wb: &mut Workbench) -> Option<RulesStatus> {
    wb.view(false).engine.session_rules.map(|s| s.status)
}

/// The panel's 确定 was taken: the engine process the rules went to.
fn applied(result: Result<CommandResult, ApiError>) -> InstanceId {
    match result.unwrap() {
        CommandResult::RulesApplied { instance, .. } => instance,
        other => panic!("not applied: {other:?}"),
    }
}

/// 中国规则 2027 > 确定: `komi 7.0` reaches KataGo before `kata-set-rules`, the game's komi is 7 and
/// the caption names the rule set once KataGo confirmed the rules.
#[test]
fn a_rule_set_sends_its_komi_before_its_rules() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    assert_eq!(wb.view(false).komi, 7.5);
    assert_eq!(applied(wb.set_kata_rules(&rules_of("CHINESE_2027"), 7.0, Some("CHINESE_2027"), false)), id);
    assert_eq!(wb.view(false).komi, 7.0);
    assert_eq!(status(&mut wb), Some(RulesStatus::Pending));
    let sent = answer_all(&mut wb, &world, id);
    let komi = sent.iter().position(|c| c == "komi 7.0");
    let rules = sent.iter().position(|c| c.starts_with("kata-set-rules "));
    assert!(komi.zip(rules).is_some_and(|(k, r)| k < r), "komi, then the rules: {sent:?}");
    assert_eq!(status(&mut wb), Some(RulesStatus::Confirmed));
    assert_eq!(caption(&mut wb), key("chinese2027"));
}

/// A handicap game changes only the rules: no `komi` command, the game keeps its komi.
#[test]
fn a_handicap_game_keeps_its_komi() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let ticket = wb.begin_open(OpenOrigin::File { path: "/handicap.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(ticket, Ok((RecordFormat::Sgf, sgf::read("(;SZ[19]HA[2]KM[0.5]AB[dp][pd])").unwrap()))));
    answer_all(&mut wb, &world, id);
    assert_eq!(wb.view(false).komi, 0.5);
    applied(wb.set_kata_rules(&rules_of("CHINESE_2027"), 7.0, Some("CHINESE_2027"), false));
    let sent = answer_all(&mut wb, &world, id);
    assert!(sent.iter().any(|c| c.starts_with("kata-set-rules ")), "{sent:?}");
    assert!(!sent.iter().any(|c| c.starts_with("komi ")), "{sent:?}");
    assert_eq!(status(&mut wb), Some(RulesStatus::Confirmed));

    // With 启动时自动应用 the picked rule set and its komi still become the defaults of new games
    // (RulesQuickSwitch); the handicap game keeps its komi.
    applied(wb.set_kata_rules(&rules_of("JAPANESE"), 6.5, Some("JAPANESE"), true));
    let sent = answer_all(&mut wb, &world, id);
    assert!(!sent.iter().any(|c| c.starts_with("komi ")), "{sent:?}");
    assert_eq!(wb.view(false).komi, 0.5);
    let c = wb.config();
    assert_eq!((c.engine.rule_set.as_deref(), c.game.komi, c.matches.komi), (Some("JAPANESE"), 6.5, 6.5));
}

/// 启动时自动应用 on: the rules are KataGo's startup rules, the rule set is the chosen one and its
/// komi that of new games, engine games and a cleared board; after a restart too.
#[test]
fn with_auto_apply_a_rule_set_becomes_the_default_of_new_games() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    applied(wb.set_kata_rules(&rules_of("CHINESE_2027"), 7.0, Some("CHINESE_2027"), true));
    answer_all(&mut wb, &world, id);
    let check = |wb: &mut Workbench| {
        let v = wb.view(false);
        assert_eq!(v.settings.engine.kata_rules, rules_of("CHINESE_2027"));
        assert!(v.settings.engine.auto_load_kata_rules);
        assert_eq!(wb.config().engine.rule_set.as_deref(), Some("CHINESE_2027"));
        assert_eq!((v.settings.game.komi, v.settings.matches.settings.komi), (7.0, 7.0), "新对局 and 引擎对局 komi");
    };
    check(&mut wb);
    wb.set_komi(6.5).unwrap();
    wb.edit(EditCommand::ClearBoard).unwrap();
    assert_eq!(wb.view(false).komi, 7.0, "a cleared board takes the rule set's komi");
    drop(wb);

    // Restarted: KataGo starts with the rules, the new game with the rule set's komi.
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    check(&mut wb);
    let v = wb.view(false);
    assert_eq!(v.komi, 7.0);
    assert_eq!(v.engine.rules.as_deref(), Some(CHINESE), "the startup rules");
    assert_eq!(caption(&mut wb), key("chinese2027"));
    wb.set_komi(6.5).unwrap();
    wb.edit(EditCommand::ClearBoard).unwrap();
    answer_all(&mut wb, &world, id);
    assert_eq!(wb.view(false).komi, 7.0);
}

/// 启动时自动应用 off: only the game and the engine change; no default does.
#[test]
fn without_auto_apply_no_default_changes() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let before = wb.config().clone();
    applied(wb.set_kata_rules(&rules_of("CHINESE_2027"), 7.0, Some("CHINESE_2027"), false));
    answer_all(&mut wb, &world, id);
    assert_eq!(caption(&mut wb), key("chinese2027"));
    let after = wb.config();
    assert_eq!(
        (&after.engine.kata_rules, after.engine.auto_load_kata_rules, &after.engine.rule_set, after.game.komi, after.matches.komi),
        (&before.engine.kata_rules, false, &None, 7.5, 7.5)
    );
    wb.edit(EditCommand::ClearBoard).unwrap();
    assert_eq!(wb.view(false).komi, 7.5, "a cleared board takes the engine's own komi");
}

/// A save that fails stops the operation: an error, and nothing reaches the engine or the game.
#[cfg(unix)]
#[test]
fn a_failed_save_reaches_no_engine() {
    use std::os::unix::fs::PermissionsExt;
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let config = dir.path().join("config");
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o555)).unwrap();
    let result = permissions::obeying_permissions(|| wb.set_kata_rules(&rules_of("CHINESE_2027"), 7.0, Some("CHINESE_2027"), true));
    std::fs::set_permissions(&config, std::fs::Permissions::from_mode(0o755)).unwrap();
    assert!(result.is_err(), "{result:?}");
    assert!(world.take(id).is_empty(), "the engine was told something");
    let v = wb.view(false);
    assert_eq!(v.komi, 7.5);
    assert_eq!(v.engine.session_rules, None);
    assert!(!wb.config().engine.auto_load_kata_rules);
}

/// KataGo refusing the rules: the view says so, so the panel stays open with the failure.
#[test]
fn refused_rules_are_reported_as_failed() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    world.refuse_rules(id);
    applied(wb.set_kata_rules(CHINESE, 7.5, None, false));
    answer_all(&mut wb, &world, id);
    let applied = wb.view(false).engine.session_rules.unwrap();
    assert_eq!(applied.status, RulesStatus::Failed);
    assert_eq!(applied.rules, rules_of("CHINESE"));
}

/// The confirmation the panel waits for reads the rules of the latest 确定, also when the rules
/// before them were confirmed already.
#[test]
fn the_published_session_rules_follow_each_choice() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    applied(wb.set_kata_rules(CHINESE, 7.5, None, false));
    answer_all(&mut wb, &world, id);
    let first = wb.view(false).engine.session_rules.unwrap();
    assert_eq!((first.rules, first.status), (rules_of("CHINESE"), RulesStatus::Confirmed));
    applied(wb.set_kata_rules(JAPANESE, 6.5, None, false));
    assert_eq!(wb.view(false).engine.session_rules.unwrap().rules, rules_of("JAPANESE"));
    answer_all(&mut wb, &world, id);
    let second = wb.view(false).engine.session_rules.unwrap();
    assert_eq!((second.rules, second.status), (rules_of("JAPANESE"), RulesStatus::Confirmed));
}

/// The komi sent before the rules is part of syncing the position: when the engine refuses it,
/// that position is not set up again by itself (no `clear_board`, no second `komi`); another
/// position is.
#[test]
fn a_refused_komi_before_the_rules_is_not_sent_again_for_the_same_position() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let ticket = wb.begin_open(OpenOrigin::File { path: "/game.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(ticket, Ok((RecordFormat::Sgf, sgf::read("(;SZ[19]KM[7.5];B[pd];W[dp])").unwrap()))));
    wb.navigate(lizzie_core::browse::Navigation::Last);
    answer_all(&mut wb, &world, id);
    world.refuse_komi(id, "komi 5.5");
    applied(wb.set_kata_rules(CHINESE, 5.5, None, false));
    let mut sent = answer_all(&mut wb, &world, id);
    for _ in 0..4 {
        wb.view(false);
        wb.engine_tick(std::time::Instant::now());
        sent.extend(answer_all(&mut wb, &world, id));
    }
    let refused = sent.iter().position(|c| c == "komi 5.5").expect("the komi was sent");
    assert!(sent[..refused].iter().all(|c| !c.starts_with("kata-set-rules")), "komi first: {sent:?}");
    let again: Vec<&String> = sent[refused + 1..].iter().filter(|c| *c == "komi 5.5" || *c == "clear_board").collect();
    assert!(again.is_empty(), "set up again: {sent:?}");
    // Another komi is another position: it is synced as usual.
    assert!(wb.set_komi(6.0).unwrap());
    let sent = answer_all(&mut wb, &world, id);
    assert!(sent.iter().any(|c| c == "komi 6.0"), "{sent:?}");
}

/// After a refused komi the engine's board is unknown; the next rules with another komi still give
/// the engine that komi before the rules (`RulesQuickSwitch.apply`): the position is set up again
/// first, then the rules are set, and the new position is analysed under them.
#[test]
fn after_a_refused_komi_the_next_rules_still_send_their_komi_first() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let ticket = wb.begin_open(OpenOrigin::File { path: "/game.sgf".into(), source: OpenSource::Drop });
    assert!(wb.complete_open(ticket, Ok((RecordFormat::Sgf, sgf::read("(;SZ[19]KM[7.5];B[pd];W[dp])").unwrap()))));
    wb.navigate(lizzie_core::browse::Navigation::Last);
    answer_all(&mut wb, &world, id);
    world.refuse_komi(id, "komi 5.5");
    applied(wb.set_kata_rules(CHINESE, 5.5, None, false));
    let first = answer_all(&mut wb, &world, id);
    assert!(first.iter().any(|c| c == "komi 5.5"), "{first:?}");
    wb.engine_tick(std::time::Instant::now());
    assert!(answer_all(&mut wb, &world, id).is_empty(), "the refused position is not tried again");
    applied(wb.set_kata_rules(JAPANESE, 6.5, None, false));
    let second = answer_all(&mut wb, &world, id);
    let at = |cmd: &str| second.iter().position(|c| c.starts_with(cmd)).unwrap_or_else(|| panic!("no {cmd}: {second:?}"));
    assert!(at("komi 6.5") < at("kata-set-rules"), "komi first: {second:?}");
    assert!(at("kata-get-rules") < at("kata-analyze"), "analysed under the rules: {second:?}");
    assert_eq!(status(&mut wb), Some(RulesStatus::Confirmed));
    assert_eq!(wb.view(false).komi, 6.5);
}

/// After a refusal, nothing is tried again by itself; 确定 once more with the same rules sets and
/// reads them back again (`SetKataRules.applySelectedRules` starts a new operation each time), and
/// until then the view does not repeat the old failure as its answer.
#[test]
fn the_same_rules_applied_again_after_a_refusal_are_sent_again() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    world.refuse_rules(id);
    applied(wb.set_kata_rules(CHINESE, 7.5, None, false));
    answer_all(&mut wb, &world, id);
    assert_eq!(status(&mut wb), Some(RulesStatus::Failed));
    wb.engine_tick(std::time::Instant::now());
    let idle = answer_all(&mut wb, &world, id);
    assert!(!idle.iter().any(|c| c.starts_with("kata-set-rules")), "no retry without 确定: {idle:?}");
    world.accept_rules(id);
    applied(wb.set_kata_rules(CHINESE, 7.5, None, false));
    assert_eq!(status(&mut wb), Some(RulesStatus::Pending));
    let sent = answer_all(&mut wb, &world, id);
    let rules: Vec<&str> = sent.iter().map(|c| c.split(' ').next().unwrap()).filter(|c| c.ends_with("-rules")).collect();
    assert_eq!(rules, ["kata-set-rules", "kata-get-rules"], "{sent:?}");
    assert_eq!(status(&mut wb), Some(RulesStatus::Confirmed));
}

/// The rules belong to the process they were given to: when it ends before reading them back,
/// the views after 确定 no longer show it as the foreground engine, and the process started in
/// its place confirms them as itself.
#[test]
fn rules_given_to_a_process_that_ended_are_not_confirmed_by_its_successor() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let (after, instance) = match wb.set_kata_rules(CHINESE, 7.5, None, false).unwrap() {
        CommandResult::RulesApplied { after, instance } => (after, instance),
        other => panic!("not applied: {other:?}"),
    };
    assert_eq!(instance, id);
    assert!(world.take(id).iter().any(|c| c.contains("kata-set-rules")));
    wb.engine_input(id, EngineInput::Exited(Some(1)));
    let v = wb.view(false);
    assert!(v.seq > after);
    assert_ne!(v.engine.foreground.map(|f| f.instance), Some(instance));
    // 自动检测引擎是否存活 starts another process, which takes the session rules.
    let next = world.last();
    assert_ne!(next, id);
    answer_all(&mut wb, &world, next);
    let v = wb.view(false);
    assert_eq!(v.engine.foreground.map(|f| f.instance), Some(next));
    assert_eq!(v.engine.session_rules.map(|s| s.status), Some(RulesStatus::Confirmed));
}

/// A stored rule set that is no text is no rule set; every other setting stays and the file is
/// not put aside as unreadable.
#[test]
fn a_stored_rule_set_that_is_no_text_is_none() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    applied(wb.set_kata_rules(&rules_of("JAPANESE"), 6.5, Some("JAPANESE"), true));
    answer_all(&mut wb, &world, id);
    let saved = wb.config().clone();
    assert_eq!(saved.engine.rule_set.as_deref(), Some("JAPANESE"));
    drop(wb);
    let file = storage(dir.path()).config_file;
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    json["engine"]["ruleSet"] = serde_json::json!(7);
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
    let wb = Workbench::new(storage(dir.path()), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    let loaded = wb.config();
    assert_eq!(loaded.engine.rule_set, None);
    assert_eq!(loaded.engine.engines, saved.engine.engines);
    assert_eq!(loaded.engine.kata_rules, saved.engine.kata_rules);
    assert!(loaded.engine.auto_load_kata_rules);
    assert_eq!((loaded.game.komi, loaded.matches.komi), (6.5, 6.5));
    let backups: Vec<_> = std::fs::read_dir(file.parent().unwrap())
        .unwrap()
        .filter_map(|e| e.ok()?.file_name().into_string().ok())
        .filter(|n| n.contains("unreadable-backup"))
        .collect();
    assert!(backups.is_empty(), "{backups:?}");
}

/// 平局时白方胜 is told from 中国规则 2002 only while it is the chosen rule set (`RulePreset.chosen`):
/// once the startup rules are others it is not chosen any more.
#[test]
fn the_chosen_rule_set_lasts_while_kata_go_starts_with_its_rules() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    let white = "CHINESE_2027_DRAW_TO_WHITE";
    applied(wb.set_kata_rules(&rules_of(white), 7.5, Some(white), true));
    answer_all(&mut wb, &world, id);
    assert_eq!(caption(&mut wb), key("chinese2027DrawToWhite"));
    assert_eq!(wb.rule_set_of(CHINESE, 7.5, None), Some(white), "the panel opens on it");
    assert_eq!(wb.rule_set_of(CHINESE, 7.0, None), Some("CHINESE_2027"));

    // The startup rules change (自定义, 启动时自动应用 on).
    applied(wb.set_kata_rules(JAPANESE, 7.5, None, true));
    answer_all(&mut wb, &world, id);
    assert_eq!(wb.config().engine.rule_set.as_deref(), Some(white), "kept, but no longer chosen");
    assert_eq!(wb.rule_set_of(CHINESE, 7.5, None), Some("CHINESE"));
    applied(wb.set_kata_rules(CHINESE, 7.5, None, false));
    answer_all(&mut wb, &world, id);
    assert_eq!(caption(&mut wb), key("chinese"));
}

/// The panel's recognition: the row picked last tells a draw rule from the rule set it counts
/// like; other rules than a rule set's, or another komi of shared rules, are 自定义.
#[test]
fn the_panel_recognises_the_rule_set_of_its_rules_and_komi() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (wb, _) = started(dir.path(), &world);
    let white = "CHINESE_2027_DRAW_TO_WHITE";
    assert_eq!(wb.rule_set_of(CHINESE, 7.0, None), Some("CHINESE_2027"));
    assert_eq!(wb.rule_set_of(CHINESE, 7.5, None), Some("CHINESE"), "2027 at 7 changed to 7.5 is 2002");
    assert_eq!(wb.rule_set_of(CHINESE, 7.5, Some(white)), Some(white));
    assert_eq!(wb.rule_set_of(CHINESE, 7.0, Some(white)), Some("CHINESE_2027"));
    assert_eq!(wb.rule_set_of(CHINESE, 6.5, None), None);
    assert_eq!(wb.rule_set_of(JAPANESE, 7.0, None), Some("JAPANESE"));
    // Chinese rules with a seki tax are no KataGo preset (positional superko would still be
    // Chinese, as OGS plays them).
    let taxed = CHINESE.replace("\"NONE\"", "\"SEKI\"");
    assert_eq!(wb.rule_set_of(&taxed, 7.0, None), None, "one rule changed: 自定义");
}

/// A rule set the rules and komi are not is refused as an argument; it changes nothing.
#[test]
fn a_rule_set_that_does_not_match_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, id) = started(dir.path(), &world);
    assert!(wb.set_kata_rules(CHINESE, 6.5, Some("CHINESE_2027"), true).is_err());
    assert!(world.take(id).is_empty());
    assert!(!wb.config().engine.auto_load_kata_rules);
}

/// Without a ready KataGo the panel's 确定 is refused with Java's message.
#[test]
fn without_kata_go_the_rules_are_not_applied() {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let mut wb = Workbench::new(storage(dir.path()), None, Box::new(FakeLauncher(world.clone())), Vec::new());
    assert_eq!(
        wb.set_kata_rules(CHINESE, 7.0, None, true).unwrap(),
        CommandResult::Refused { message_key: "LizzieFrame.setParamNoEngineHint".into() }
    );
    assert!(!wb.config().engine.auto_load_kata_rules);
}
