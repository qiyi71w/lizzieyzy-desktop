//! 终局点目's verdict and the rules caption through the public `Workbench` operations, with a
//! controlled KataGo reporting chosen rules: equal totals are a draw (or the first passer's under
//! KataGo's button rules), whole margins have no ".0", the result written into `RE` follows,
//! territory rules keep their name whatever `friendlyPassOk` says, and the caption names the 2027
//! Chinese rule set the rules and the game's komi are.

use std::sync::{Arc, Mutex};

use lizzie_core::api::{ScoreCommand, ScoreVerdict, ScoreView};
use lizzie_core::browse::Navigation;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::formats::{self, sgf};
use lizzie_core::record::{RecordFormat, Stone};
use lizzie_core::settings::Storage;
use lizzie_core::workbench::{OpenOrigin, Workbench};

/// Commands each instance was sent and not yet answered.
type Sent = Vec<(InstanceId, Vec<String>)>;

#[derive(Clone, Default)]
struct World(Arc<Mutex<Sent>>);

impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        let mut w = self.0.lock().unwrap();
        w.iter_mut()
            .find(|p| p.0 == id)
            .map(|p| std::mem::take(&mut p.1))
            .unwrap_or_default()
    }
}

struct FakeIo(InstanceId, World);

impl EngineIo for FakeIo {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        let mut w = (self.1).0.lock().unwrap();
        w.iter_mut().find(|p| p.0 == self.0).unwrap().1.push(line.to_string());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}

struct FakeLauncher(World);

impl Launcher for FakeLauncher {
    fn launch(&self, id: InstanceId, _spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push((id, Vec::new()));
        Ok(Box::new(FakeIo(id, self.0.clone())))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\nkata-analyze\nkata-set-rules\nkata-get-rules\nstop";

/// KataGo's `kata-get-rules` answers (1.18.1 field order does not matter).
const CHINESE: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
/// Chinese rules with the button (数子规则下收后方贴还 0.5 目: 是).
const CHINESE_BUTTON: &str = r#"{"friendlyPassOk":true,"hasButton":true,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

fn out(wb: &mut Workbench, id: InstanceId, line: &str) {
    wb.engine_input(id, EngineInput::Stdout(line.to_string()));
}

/// Answers every outstanding command, reporting `rules` to `kata-get-rules`; analysis requests
/// get only their header.
fn answer_all(wb: &mut Workbench, world: &World, id: InstanceId, rules: &str) {
    loop {
        let pending = world.take(id);
        if pending.is_empty() {
            return;
        }
        for line in pending {
            out(wb, id, "");
            let (n, cmd) = line.split_once(' ').unwrap();
            let reply = match cmd.split(' ').next().unwrap() {
                "name" => Some("KataGo"),
                "version" => Some("1.18.1"),
                "list_commands" => Some(KATA),
                "kata-get-rules" => Some(rules),
                "kata-analyze" => None,
                _ => Some(""),
            };
            match reply {
                Some(text) => {
                    out(wb, id, &format!("={n} {text}"));
                    out(wb, id, "");
                }
                None => out(wb, id, &format!("={n}")),
            }
        }
    }
}

/// A workbench whose foreground KataGo reports `rules`.
fn started(dir: &std::path::Path, world: &World, rules: &str) -> (Workbench, InstanceId) {
    let storage = Storage::in_dirs(&dir.join("config"), &dir.join("data"));
    let mut wb = Workbench::new(storage, None, Box::new(FakeLauncher(world.clone())), Vec::new());
    wb.set_auto_quick_analyze(false).unwrap();
    let profile = EngineProfile {
        name: "KataGo".into(),
        command: "/opt/katago gtp".into(),
        ..EngineProfile::default()
    };
    wb.save_engine_settings(vec![profile], EngineStartup::Default).unwrap();
    wb.start_engines();
    let id = world.0.lock().unwrap()[0].0;
    answer_all(&mut wb, world, id, rules);
    (wb, id)
}

/// 7x7: Black's stones on the d column own a-d (28 points), White's on the e column own e-g
/// (21 points); `moves` follow the setup and the position is scored after them.
fn count(dir: &std::path::Path, rules: &str, komi: &str, moves: &str) -> (Workbench, ScoreView) {
    let world = World::default();
    let (mut wb, id) = started(dir, &world, rules);
    let text = format!("(;SZ[7]KM[{komi}]AB[da][db][dc][dd][de][df][dg]AW[ea][eb][ec][ed][ee][ef][eg]{moves})");
    let t = wb.begin_open(OpenOrigin::File {
        path: "/count.sgf".into(),
        source: lizzie_core::api::OpenSource::Drop,
    });
    assert!(wb.complete_open(t, Ok((RecordFormat::Sgf, sgf::read(&text).unwrap()))));
    answer_all(&mut wb, &world, id, rules);
    wb.navigate(Navigation::Forward(10));
    answer_all(&mut wb, &world, id, rules);
    wb.score(ScoreCommand::Toggle).unwrap();
    let s = wb.view(false).score.expect("scoring");
    (wb, s)
}

fn win(winner: Stone, margin: &str) -> ScoreVerdict {
    ScoreVerdict::Win {
        winner,
        margin: margin.into(),
    }
}

/// Equal totals under a whole-point komi (the 2027 Chinese rules' 7) are a draw, not a win for
/// Black by 0.0; confirming writes the SGF draw `0`, which a saved and reopened record keeps.
#[test]
fn equal_totals_are_a_draw_written_as_a_draw() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, s) = count(dir.path(), CHINESE, "7", "");
    assert_eq!((s.black_total, s.white_total), (28.0, 28.0));
    assert_eq!((s.verdict, s.result.as_str()), (ScoreVerdict::Draw, "0"));

    wb.score(ScoreCommand::Confirm).unwrap();
    assert_eq!(wb.document().record.info.result, "0");
    let path = dir.path().join("draw.sgf");
    let job = wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
    let written = formats::write_record_file(&job.path, &job.text).map_err(|e| e.to_string());
    assert!(wb.complete_save(job.ticket, written));
    assert_eq!(formats::read_file(&path, false).unwrap().1.info.result, "0");
}

/// A whole margin has no ".0" on screen or in `RE`; a half point keeps its decimal; the side with
/// more wins as before.
#[test]
fn margins_are_written_without_a_trailing_zero() {
    let dir = tempfile::tempdir().unwrap();
    let (_, s) = count(dir.path(), CHINESE, "5", "");
    assert_eq!((s.verdict, s.result.as_str()), (win(Stone::Black, "2"), "B+2"));
    let (_, s) = count(dir.path(), CHINESE, "7.5", "");
    assert_eq!((s.verdict, s.result.as_str()), (win(Stone::White, "0.5"), "W+0.5"));
    let (_, s) = count(dir.path(), CHINESE, "6.5", "");
    assert_eq!((s.verdict, s.result.as_str()), (win(Stone::Black, "0.5"), "B+0.5"));
    let (_, s) = count(dir.path(), CHINESE, "10", "");
    assert_eq!((s.verdict, s.result.as_str()), (win(Stone::White, "3"), "W+3"));
}

/// Under KataGo's button rules a drawn count goes to the player who passed first on the way to
/// the position, as a win by no margin (`B+`); with nobody having passed it stays a draw, and
/// other counts are decided by their margin as usual.
#[test]
fn button_rules_give_a_draw_to_the_first_passer() {
    let dir = tempfile::tempdir().unwrap();
    let (mut wb, s) = count(dir.path(), CHINESE_BUTTON, "7", ";B[];W[]");
    assert_eq!(
        (s.verdict, s.result.as_str()),
        (ScoreVerdict::DrawToFirstPasser { winner: Stone::Black }, "B+")
    );
    wb.score(ScoreCommand::Confirm).unwrap();
    assert_eq!(wb.document().record.info.result, "B+");

    let (_, s) = count(dir.path(), CHINESE_BUTTON, "7", ";W[];B[];W[]");
    assert_eq!(
        (s.verdict, s.result.as_str()),
        (ScoreVerdict::DrawToFirstPasser { winner: Stone::White }, "W+")
    );

    let (_, s) = count(dir.path(), CHINESE_BUTTON, "7", "");
    assert_eq!((s.verdict, s.result.as_str()), (ScoreVerdict::Draw, "0"));

    let (_, s) = count(dir.path(), CHINESE_BUTTON, "5", ";W[];B[]");
    assert_eq!((s.verdict, s.result.as_str()), (win(Stone::Black, "2"), "B+2"));

    // Without the button the first pass decides nothing.
    let (_, s) = count(dir.path(), CHINESE, "7", ";W[];B[]");
    assert_eq!((s.verdict, s.result.as_str()), (ScoreVerdict::Draw, "0"));
}

fn caption(rules: &str) -> Option<String> {
    let dir = tempfile::tempdir().unwrap();
    let world = World::default();
    let (mut wb, _) = started(dir.path(), &world, rules);
    wb.view(false).engine.rules_key
}

/// KataGo reports `friendlyPassOk` false for Japanese and Korean rules (which are the same rules
/// to it); territory scoring does not use the field, so either value is 日本规则. Rules that
/// really differ stay 其他规则, and area scoring still compares the field.
#[test]
fn japanese_and_korean_rules_are_named_whatever_friendly_pass_says() {
    let territory = |friendly: bool, tax: &str, button: bool| {
        format!(
            r#"{{"friendlyPassOk":{friendly},"hasButton":{button},"ko":"SIMPLE","scoring":"TERRITORY","suicide":false,"tax":"{tax}","whiteHandicapBonus":"0"}}"#
        )
    };
    let japanese = Some("LizzieFrame.currentRules.japanese".to_string());
    let others = Some("LizzieFrame.currentRules.others".to_string());
    // Japanese and Korean: the same report, either friendlyPassOk.
    assert_eq!(caption(&territory(false, "SEKI", false)), japanese);
    assert_eq!(caption(&territory(true, "SEKI", false)), japanese);
    // Territory rules that differ in a field that counts.
    assert_eq!(caption(&territory(false, "SEKI", true)), others);
    assert_eq!(caption(&territory(true, "NONE", false)), others);
    // Area scoring uses friendly passes: Chinese without them is not Chinese.
    assert_eq!(
        caption(&CHINESE.replace(r#""friendlyPassOk":true"#, r#""friendlyPassOk":false"#)),
        others
    );
    assert_eq!(caption(CHINESE), Some("LizzieFrame.currentRules.chinese".to_string()));
}

/// The caption follows the komi as well as the rules (`RulePreset.captionKey`): Chinese rules
/// with komi 7 are 中国规则2027, with 7.5 中国规则 (2002), with a komi of no rule set still
/// 中国规则; the button rules are 2027·平局先停胜 whatever the komi.
#[test]
fn the_caption_follows_the_komi() {
    let dir = tempfile::tempdir().unwrap();
    let caption_at = |rules: &str, komis: &[f64]| -> Vec<Option<String>> {
        let world = World::default();
        let (mut wb, id) = started(dir.path(), &world, rules);
        komis
            .iter()
            .map(|&komi| {
                wb.set_komi(komi).unwrap();
                answer_all(&mut wb, &world, id, rules);
                wb.view(false).engine.rules_key
            })
            .collect()
    };
    let key = |k: &str| Some(format!("LizzieFrame.currentRules.{k}"));
    assert_eq!(
        caption_at(CHINESE, &[7.0, 7.5, 6.5, 7.0]),
        [key("chinese2027"), key("chinese"), key("chinese"), key("chinese2027")]
    );
    assert_eq!(
        caption_at(CHINESE_BUTTON, &[7.0, 7.5, 0.5]),
        [
            key("chinese2027DrawToFirstPass"),
            key("chinese2027DrawToFirstPass"),
            key("chinese2027DrawToFirstPass")
        ]
    );
}
