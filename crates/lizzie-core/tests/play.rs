//! 人机对弈 with a controlled GTP engine: the engine reservation, the engine-move transaction and
//! its stale answers, the end of a game and the engine's restore.
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

use lizzie_core::api::{
    Command, CommandResult, EditCommand, GameCommand, GameEnd, GameKind, GamePhase, LeaseAvailabilityView, OpenSource,
    ScoreCommand,
};
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::record::{NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{AiTimeMode, AutoPlaySettings, GameSettings, Storage};
use lizzie_core::workbench::Workbench;

#[derive(Default)]
struct Process {
    id: InstanceId,
    sent: Vec<String>,
}
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Process>>>);
impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
}
struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().sent.push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}
struct Launch(World);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push(Process { id, ..Process::default() });
        Ok(Box::new(Io(self.0.clone(), id)))
    }
}

const KATA: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nkata-analyze\n\
kata-set-rules\nkata-get-rules\nkata-get-param\nkata-set-param\nkata-time_settings\ngenmove\nfixed_handicap\n\
place_free_handicap\nkata-raw-nn\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;

struct Game {
    wb: Workbench,
    world: World,
    engine: InstanceId,
    /// Engine requests left unanswered: (gtp id, command).
    held: Vec<(String, String)>,
    /// A `kata-analyze` stream is open; the next command ends it as KataGo does.
    analyzing: bool,
    _dir: tempfile::TempDir,
}

fn output(wb: &mut Workbench, id: InstanceId, text: &str) {
    wb.engine_input(id, EngineInput::Stdout(text.into()));
}

impl Game {
    fn new() -> Game {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            None,
            Box::new(Launch(world.clone())),
            vec![],
        );
        wb.set_auto_quick_analyze(false).unwrap();
        wb.save_engine_settings(
            vec![EngineProfile { name: "KataGo".into(), command: "/opt/katago gtp".into(), ..EngineProfile::default() }],
            EngineStartup::Default,
        )
        .unwrap();
        wb.start_engines();
        let engine = world.0.lock().unwrap().last().unwrap().id;
        let mut g = Game { wb, world, engine, held: Vec::new(), analyzing: false, _dir: dir };
        g.serve(&|_| None);
        g
    }

    /// Answers everything the engine was sent and returns the commands. `genmove` is always held;
    /// for other commands `answer` may give the reply text, or "HOLD" to hold it, else the
    /// default reply is sent.
    fn serve(&mut self, answer: &dyn Fn(&str) -> Option<String>) -> Vec<String> {
        let mut seen = Vec::new();
        loop {
            let pending = self.world.take(self.engine);
            if pending.is_empty() {
                return seen;
            }
            for line in pending {
                let (id, cmd) = line.split_once(' ').unwrap();
                seen.push(cmd.to_string());
                if std::mem::take(&mut self.analyzing) {
                    output(&mut self.wb, self.engine, "");
                }
                if cmd.starts_with("genmove") {
                    self.held.push((id.to_string(), cmd.to_string()));
                    continue;
                }
                if let Some(text) = answer(cmd) {
                    if text == "HOLD" {
                        self.held.push((id.to_string(), cmd.to_string()));
                        continue;
                    }
                    self.reply(id, &text);
                    continue;
                }
                let text = match cmd.split(' ').next().unwrap() {
                    "name" => "KataGo",
                    "version" => "1.18.2",
                    "list_commands" => KATA,
                    "kata-get-rules" => RULES,
                    "kata-get-param" => "1.5",
                    _ => "",
                };
                if cmd.starts_with("kata-analyze") {
                    output(&mut self.wb, self.engine, &format!("={id}"));
                    self.analyzing = true;
                    continue;
                }
                self.reply(id, text);
            }
        }
    }

    /// A success, or a GTP failure (`?id …`) for text starting with `?`.
    fn reply(&mut self, id: &str, text: &str) {
        match text.strip_prefix('?') {
            Some(error) => output(&mut self.wb, self.engine, &format!("?{id} {error}")),
            None => output(&mut self.wb, self.engine, &format!("={id} {text}")),
        }
        output(&mut self.wb, self.engine, "");
    }

    /// Answers the held request starting with `prefix`.
    fn answer_held(&mut self, prefix: &str, text: &str) {
        let at = self.held.iter().position(|(_, c)| c.starts_with(prefix)).unwrap_or_else(|| panic!("no held {prefix}: {:?}", self.held));
        let (id, _) = self.held.remove(at);
        self.reply(&id, text);
    }

    fn game(&mut self, op: GameCommand) -> CommandResult {
        self.wb.game(op).unwrap()
    }

    fn phase(&mut self) -> Option<GamePhase> {
        self.wb.view(false).game.map(|g| g.phase)
    }
}

fn genmove_settings(human_black: bool) -> GameSettings {
    GameSettings { play_black: human_black, auto_save: false, ..GameSettings::default() }
}

fn start(g: &mut Game, kind: GameKind, settings: GameSettings, autoplay: Option<AutoPlaySettings>) {
    assert_eq!(g.game(GameCommand::Start { kind, settings, autoplay, engine: None }), CommandResult::Done);
}

fn play(g: &mut Game, x: u16, y: u16) -> CommandResult {
    g.wb.edit(EditCommand::Play { point: Point { x, y }, color: None }).unwrap()
}

fn last_move(wb: &Workbench) -> Option<(Stone, Option<Point>)> {
    match wb.document().record.node(wb.cursor()).kind {
        NodeKind::Move { color, point } => Some((color, Some(point))),
        NodeKind::Pass { color } => Some((color, None)),
        _ => None,
    }
}

#[test]
fn an_accepted_game_has_not_started_until_its_engine_setup_is_answered() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    assert_eq!(g.phase(), Some(GamePhase::Starting));
    let early = play(&mut g, 3, 3);
    assert_eq!(early, CommandResult::Refused { message_key: "LizzieFrame.waitEngineLoadingHint".into() });
    assert_eq!(g.wb.cursor(), ROOT, "no move before the game started");
    // The setup is sent but not yet answered: still not started.
    let sent = g.serve(&|c| c.starts_with("kata-set-param maxTime").then(|| "HOLD".to_string()));
    assert!(sent.iter().any(|c| c == "kata-get-param maxTime"), "{sent:?}");
    assert_eq!(g.phase(), Some(GamePhase::Starting));
    g.answer_held("kata-set-param maxTime", "");
    assert_eq!(g.phase(), Some(GamePhase::Playing));
    assert_eq!(play(&mut g, 3, 3), CommandResult::Done);
    g.serve(&|_| None);
    assert!(g.held.iter().any(|(_, c)| c == "genmove W"), "the engine is asked for White's move: {:?}", g.held);
}

#[test]
fn the_engine_move_is_committed_once_for_the_position_it_answers() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(false), None);
    g.serve(&|_| None);
    assert!(g.held.iter().any(|(_, c)| c == "genmove B"), "the engine opens as Black: {:?}", g.held);
    assert_eq!(play(&mut g, 3, 3), CommandResult::Unchanged, "not the human's turn");
    g.answer_held("genmove B", "Q16");
    let at = g.wb.cursor();
    assert_eq!(last_move(&g.wb), Some((Stone::Black, Some(Point { x: 15, y: 3 }))));
    // The committed move is the engine board's move: the next sync adds no undo/replay.
    assert_eq!(play(&mut g, 3, 3), CommandResult::Done);
    let sent = g.serve(&|_| None);
    assert!(!sent.iter().any(|c| c.starts_with("undo") || c.starts_with("clear_board")), "{sent:?}");
    assert!(sent.iter().any(|c| c == "play W D16"), "{sent:?}");
    assert_ne!(g.wb.cursor(), at);
}

#[test]
fn a_late_engine_move_after_regret_changes_nothing() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    let revision = g.wb.document().revision;
    assert_eq!(g.game(GameCommand::Regret), CommandResult::Done);
    assert_eq!(g.wb.cursor(), ROOT);
    g.answer_held("genmove W", "Q16");
    assert_eq!(g.wb.cursor(), ROOT, "the answer is for a position no longer shown");
    assert_eq!(g.wb.document().revision, revision);
    assert_eq!(g.wb.document().record.node(ROOT).children.len(), 1, "no engine stone was added anywhere");
}

#[test]
fn a_late_engine_move_after_the_user_ends_the_game_changes_nothing_and_blocks_until_restored() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    let (doc, cursor, revision) = (g.wb.document().id, g.wb.cursor(), g.wb.document().revision);
    assert_eq!(g.game(GameCommand::End), CommandResult::Done);
    let ending = g.wb.view(false);
    assert_eq!(ending.game.map(|v| v.phase), Some(GamePhase::Ending));
    assert_eq!(ending.game_outcome.map(|o| o.end), Some(GameEnd::Terminated));
    let open = Command::OpenRecord { path: "/x.sgf".into(), source: OpenSource::Drop };
    assert_eq!(g.wb.game_gate(&open), Some(CommandResult::Refused { message_key: "LizzieFrame.openFileFailed.inGame".into() }));
    assert_eq!(
        g.wb.game_gate(&Command::SwitchEngine { index: 0 }),
        Some(CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.play_mode".into() })
    );
    assert_eq!(g.wb.view(false).engine.lease, LeaseAvailabilityView::PlayMode);
    // The restore (the captured maxTime) is sent behind the move in flight, then the `name` fence.
    let sent = g.serve(&|c| (c == "name").then(|| "HOLD".to_string()));
    assert!(sent.iter().any(|c| c == "kata-set-param maxTime 1.5"), "{sent:?}");
    g.answer_held("genmove W", "Q16");
    assert_eq!((g.wb.document().id, g.wb.cursor(), g.wb.document().revision), (doc, cursor, revision));
    assert_eq!(g.phase(), Some(GamePhase::Ending), "still blocked until the restore is confirmed");
    assert!(g.wb.game_gate(&open).is_some());
    g.answer_held("name", "KataGo");
    assert_eq!(g.phase(), None);
    assert_eq!(g.wb.game_gate(&open), None);
    assert_ne!(g.wb.view(false).engine.lease, LeaseAvailabilityView::PlayMode);
    assert!(g.wb.view(false).engine.paused, "analysis stays paused after a game");
}

#[test]
fn a_new_record_ends_the_game_and_its_late_engine_move_lands_nowhere() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    let old = g.wb.document().id;
    assert_eq!(g.wb.edit(EditCommand::ClearBoard).unwrap(), CommandResult::Done);
    assert_ne!(g.wb.document().id, old);
    assert_eq!(g.wb.view(false).game_outcome.map(|o| o.end), Some(GameEnd::Terminated));
    g.serve(&|_| None);
    g.answer_held("genmove W", "Q16");
    g.serve(&|_| None);
    assert_eq!(g.wb.cursor(), ROOT);
    assert!(g.wb.document().record.node(ROOT).children.is_empty(), "the old game's move is not in the new record");
    assert_eq!(g.phase(), None, "the engine was given back");
}

#[test]
fn an_unconfirmed_restore_leaves_the_engine_unrestored() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    g.game(GameCommand::End);
    g.serve(&|c| (c == "name").then(|| "HOLD".to_string()));
    assert_eq!(g.phase(), Some(GamePhase::Ending));
    g.wb.engine_tick(Instant::now() + Duration::from_secs(120));
    assert_eq!(g.phase(), None);
    assert_eq!(g.wb.view(false).engine.lease, LeaseAvailabilityView::EngineStateUnrestored);
    let again = g.game(GameCommand::Start { kind: GameKind::Genmove, settings: genmove_settings(true), autoplay: None, engine: None });
    assert_eq!(again, CommandResult::Refused { message_key: "AnalysisSettings.reuseStatus.engine_state_unrestored".into() });
}

#[test]
fn an_engine_exit_retires_the_game_and_a_late_move_has_no_target() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    g.wb.engine_input(g.engine, EngineInput::Exited(Some(1)));
    let v = g.wb.view(false);
    assert!(v.game.is_none(), "nothing is left to restore");
    let outcome = v.game_outcome.expect("the game ended");
    assert_eq!(outcome.end, GameEnd::Retired);
    assert_eq!(outcome.result, None, "a retired game records no result");
    assert_eq!(last_move(&g.wb), Some((Stone::Black, Some(Point { x: 3, y: 3 }))));
}

#[test]
fn resignations_record_the_winner_and_the_game_is_saved_to_my_games() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, GameSettings { play_black: true, ..GameSettings::default() }, None);
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    g.answer_held("genmove W", "resign");
    let v = g.wb.view(false);
    let outcome = v.game_outcome.unwrap();
    assert_eq!((outcome.end, outcome.result.as_deref()), (GameEnd::EngineResigned, Some("B+R")));
    assert!(v.game_notices.iter().any(|n| n.message_key == "Leelaz.blackWinAiResign"));
    assert_eq!(g.wb.document().record.info.result, "B+R");
    let jobs = g.wb.take_background_saves();
    assert_eq!(jobs.len(), 1);
    assert!(jobs[0].create_parent && jobs[0].path.parent().unwrap().ends_with("MyGames"), "{:?}", jobs[0].path);
    assert!(jobs[0].text.contains("RE[B+R]"));
    g.serve(&|_| None);
    assert_eq!(g.phase(), None);

    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|_| None);
    assert_eq!(g.game(GameCommand::Resign), CommandResult::Done);
    assert_eq!(g.wb.document().record.info.result, "W+R", "the human resigned as Black");
    assert_eq!(g.wb.view(false).game_outcome.map(|o| o.end), Some(GameEnd::HumanResigned));
}

#[test]
fn a_handicap_game_takes_the_engines_stones_and_white_moves_first() {
    let mut g = Game::new();
    let settings = GameSettings { handicap: 2, ai_time: lizzie_core::settings::AiTimeSettings { mode: AiTimeMode::EngineOwned, ..Default::default() }, ..genmove_settings(true) };
    start(&mut g, GameKind::Genmove, settings, None);
    let sent = g.serve(&|c| c.starts_with("fixed_handicap").then(|| "D4 Q16".to_string()));
    assert!(sent.iter().any(|c| c == "fixed_handicap 2"), "{sent:?}");
    assert!(!sent.iter().any(|c| c.contains("time_settings") || c.contains("maxTime")), "engine-owned time sends nothing");
    let rec = &g.wb.document().record;
    assert_eq!(rec.info.handicap, 2);
    assert_eq!(rec.node(ROOT).to_play, Stone::White);
    assert_eq!(rec.position(ROOT).stones.iter().flatten().count(), 2);
    assert_eq!(g.phase(), Some(GamePhase::Playing));
    assert!(g.held.iter().any(|(_, c)| c == "genmove W"), "the engine (White) moves first: {:?}", g.held);
}

#[test]
fn an_analysis_game_plays_when_its_condition_is_met_and_resigns_when_lost() {
    let mut g = Game::new();
    let autoplay = AutoPlaySettings { time_enabled: false, playouts_enabled: true, playouts: 100, ..AutoPlaySettings::default() };
    let settings = GameSettings { resign_start_move: 1, resign_move: 2, resign_percent: 10.0, ..genmove_settings(true) };
    start(&mut g, GameKind::Analysis, settings, Some(autoplay));
    g.serve(&|_| None);
    assert_eq!(g.phase(), Some(GamePhase::Playing));
    play(&mut g, 3, 3);
    let sent = g.serve(&|_| None);
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "the engine thinks on its turn: {sent:?}");
    let id = g.engine;
    output(&mut g.wb, id, &info("Q16", 50, 0.5));
    assert_eq!(last_move(&g.wb), Some((Stone::Black, Some(Point { x: 3, y: 3 }))), "not enough playouts yet");
    output(&mut g.wb, id, &info("Q16", 150, 0.5));
    assert_eq!(last_move(&g.wb), Some((Stone::White, Some(Point { x: 15, y: 3 }))));
    // Two consecutive decisions below 10% for the engine: it resigns.
    play(&mut g, 3, 15);
    g.serve(&|_| None);
    output(&mut g.wb, id, &info("C3", 150, 0.05));
    assert_eq!(g.wb.view(false).game.map(|v| v.phase), Some(GamePhase::Playing), "one losing decision is not enough");
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::White), "the engine still plays");
    play(&mut g, 15, 15);
    g.serve(&|_| None);
    output(&mut g.wb, id, &info("R3", 150, 0.05));
    let v = g.wb.view(false);
    let outcome = v.game_outcome.expect("the engine resigned");
    assert_eq!((outcome.end, outcome.result.as_deref()), (GameEnd::EngineResigned, Some("B+R")));
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::Black), "no move with the resignation");
}

fn info(at: &str, visits: u32, winrate: f64) -> String {
    format!("info move {at} visits {visits} winrate {winrate} scoreLead 0.5 prior 0.2 order 0 pv {at} rootInfo visits {visits} winrate {winrate} scoreLead 0.5")
}

#[test]
fn a_refused_restore_leaves_the_engine_unrestored() {
    for refused in ["kata-set-param maxTime 1.5", "name"] {
        let mut g = Game::new();
        start(&mut g, GameKind::Genmove, genmove_settings(true), None);
        g.serve(&|_| None);
        g.game(GameCommand::End);
        g.serve(&|c| (c == refused).then(|| "?unknown command".to_string()));
        assert_eq!(g.phase(), None, "the game is over ({refused})");
        assert_eq!(g.wb.view(false).engine.lease, LeaseAvailabilityView::EngineStateUnrestored, "{refused} refused");
    }
}

#[test]
fn a_game_ended_while_its_setup_is_unanswered_restores_what_the_setup_changed() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(true), None);
    g.serve(&|c| (c == "kata-get-param maxTime").then(|| "HOLD".to_string()));
    assert_eq!(g.phase(), Some(GamePhase::Starting));
    g.game(GameCommand::End);
    let before = g.serve(&|_| None);
    assert!(!before.iter().any(|c| c == "name"), "no release before the setup is answered: {before:?}");
    assert_eq!(g.phase(), Some(GamePhase::Ending));
    g.answer_held("kata-get-param maxTime", "1.5");
    let after = g.serve(&|_| None);
    let restore = after.iter().position(|c| c == "kata-set-param maxTime 1.5");
    let name = after.iter().position(|c| c == "name");
    assert!(restore.is_some() && restore < name, "the read value is restored before the fence: {after:?}");
    assert_eq!(g.phase(), None);
    assert_ne!(g.wb.view(false).engine.lease, LeaseAvailabilityView::EngineStateUnrestored);
}

#[test]
fn a_late_handicap_answer_leaves_a_new_record_alone() {
    let mut g = Game::new();
    let settings = GameSettings { handicap: 2, ..genmove_settings(true) };
    start(&mut g, GameKind::Genmove, settings, None);
    g.serve(&|c| c.starts_with("fixed_handicap").then(|| "HOLD".to_string()));
    g.wb.edit(EditCommand::ClearBoard).unwrap();
    let (doc, revision) = (g.wb.document().id, g.wb.document().revision);
    g.answer_held("fixed_handicap", "D4 Q16");
    g.serve(&|_| None);
    let rec = &g.wb.document().record;
    assert_eq!((g.wb.document().id, g.wb.document().revision), (doc, revision));
    assert_eq!(rec.position(ROOT).stones.iter().flatten().count(), 0);
    assert_eq!((rec.info.handicap, rec.node(ROOT).to_play), (0, Stone::Black));
    assert_eq!(g.phase(), None);
}

#[test]
fn an_ended_game_whose_engine_never_answers_can_be_released_by_restarting_it() {
    let mut g = Game::new();
    start(&mut g, GameKind::Genmove, genmove_settings(false), None);
    g.serve(&|_| None);
    assert!(g.held.iter().any(|(_, c)| c == "genmove B"));
    assert!(g.wb.game_gate(&Command::RestartEngine).is_some(), "a running game keeps its engine");
    g.game(GameCommand::End);
    assert_eq!(g.wb.game_gate(&Command::RestartEngine), None, "an ending game's engine may be restarted");
    g.wb.restart_engine().unwrap();
    assert_eq!(g.phase(), None, "the stuck engine is gone with the game");
}

#[test]
fn an_analysis_game_uses_the_bottom_bar_autoplay_and_clears_it_at_the_end() {
    let mut g = Game::new();
    let autoplay = AutoPlaySettings { time_enabled: false, playouts_enabled: true, playouts: 100, ..AutoPlaySettings::default() };
    start(&mut g, GameKind::Analysis, genmove_settings(true), Some(autoplay));
    g.serve(&|_| None);
    let a = g.wb.view(false).settings.autoplay;
    assert!(a.enabled && !a.settings.black && a.settings.white, "自动落子 on for the engine's White: {a:?}");
    play(&mut g, 3, 3);
    g.serve(&|_| None);
    let off = AutoPlaySettings { white: true, ..a.settings };
    g.game(GameCommand::SetAutoPlay { enabled: false, settings: off });
    g.serve(&|_| None);
    output(&mut g.wb, g.engine, &info("Q16", 150, 0.5));
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::Black), "自动落子 off: the engine does not move");
    g.game(GameCommand::SetAutoPlay { enabled: true, settings: off });
    g.serve(&|_| None);
    // 终局点目 stops the game's analysis and its moves too.
    g.wb.score(ScoreCommand::Toggle).unwrap();
    output(&mut g.wb, g.engine, &info("Q16", 150, 0.5));
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::Black), "no move while counting");
    g.wb.score(ScoreCommand::Toggle).unwrap();
    g.serve(&|_| None);
    output(&mut g.wb, g.engine, &info("Q16", 150, 0.5));
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::White));
    g.game(GameCommand::End);
    g.serve(&|_| None);
    let a = g.wb.view(false).settings.autoplay;
    assert!(!a.enabled && !a.settings.black && !a.settings.white, "cleared after the game: {a:?}");
}

#[test]
fn a_pure_network_display_does_not_keep_an_analysis_game_from_analysing() {
    let mut g = Game::new();
    assert!(g.wb.toggle_raw_net());
    g.serve(&|_| None);
    let autoplay = AutoPlaySettings { time_enabled: false, playouts_enabled: true, playouts: 100, ..AutoPlaySettings::default() };
    start(&mut g, GameKind::Analysis, genmove_settings(true), Some(autoplay));
    g.serve(&|_| None);
    play(&mut g, 3, 3);
    let sent = g.serve(&|_| None);
    assert!(sent.iter().any(|c| c.starts_with("kata-analyze")), "{sent:?}");
    output(&mut g.wb, g.engine, &info("Q16", 150, 0.5));
    assert_eq!(last_move(&g.wb).map(|m| m.0), Some(Stone::White));
}
