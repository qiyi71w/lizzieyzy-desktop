//! One record going from review into each play/training mode and back, through the same command
//! gate (`Workbench::game_gate`) and operations the UI uses, with controlled GTP and analysis
//! processes. The per-mode suites prove each mode alone; these chains prove that a mode's
//! reservation, late answers and pause state do not leak into the next mode or into review.
use lizzie_core::api::*;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::record::{NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{self, Config, GameSettings, HumanSlTrainingSettings, MatchSettings, PlayerColor, Storage};
use lizzie_core::workbench::Workbench;
use serde_json::{json, Value};
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

// ---------------------------------------------------------------------------- controlled processes

#[derive(Default)]
struct Process {
    id: InstanceId,
    analysis: bool,
    sent: Vec<String>,
    dead: bool,
}
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Process>>>);
struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().sent.push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {
        self.0 .0.lock().unwrap().iter_mut().find(|p| p.id == self.1).unwrap().dead = true;
    }
}
impl Launcher for World {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        let analysis = spec.args.first().is_some_and(|a| a == "analysis");
        self.0.lock().unwrap().push(Process { id, analysis, ..Process::default() });
        Ok(Box::new(Io(self.clone(), id)))
    }
}
impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn gtp(&self) -> Vec<InstanceId> {
        self.0.lock().unwrap().iter().filter(|p| !p.analysis && !p.dead).map(|p| p.id).collect()
    }
    fn worker(&self) -> InstanceId {
        self.0.lock().unwrap().iter().rev().find(|p| p.analysis).unwrap().id
    }
    fn dead(&self, id: InstanceId) -> bool {
        self.0.lock().unwrap().iter().find(|p| p.id == id).unwrap().dead
    }
}

const COMMANDS: &str = "name\nversion\nlist_commands\nkomi\nboardsize\nclear_board\nplay\nundo\nset_position\n\
kata-analyze\nkata-set-rules\nkata-get-rules\nkata-get-param\nkata-set-param\nkata-time_settings\ngenmove\n\
fixed_handicap\nplace_free_handicap\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
const D4: Point = Point { x: 3, y: 15 };

struct Bench {
    wb: Workbench,
    world: World,
    /// Unanswered GTP requests: (engine, gtp id, command).
    held: Vec<(InstanceId, String, String)>,
    streams: HashSet<InstanceId>,
    _dir: tempfile::TempDir,
}

impl Bench {
    /// Two GTP engines (the first starts as the foreground engine), an analysis engine command
    /// and a HumanSL model for AI 陪练.
    fn new() -> Bench {
        let dir = tempfile::tempdir().unwrap();
        let storage = Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
        let mut config = Config::default();
        config.analysis_engine.command = "/fake/katago analysis -config /fake/analysis.cfg -model /fake/main.bin.gz".into();
        config.analysis_engine.command_customized = true;
        let model = dir.path().join("custom-human.bin.gz");
        std::fs::File::create(&model).unwrap().set_len(1024 * 1024 + 1).unwrap();
        config.human_sl.model_path = Some(model.to_string_lossy().into_owned());
        settings::save_json(&storage.config_file, &config).unwrap();
        let world = World::default();
        let mut wb = Workbench::new(storage, None, Box::new(world.clone()), vec![]);
        wb.set_human_sl_random_seed(7);
        wb.set_auto_quick_analyze(false).unwrap();
        wb.save_engine_settings(
            vec![
                EngineProfile { id: "one".into(), name: "First".into(), command: "/opt/katago gtp".into(), ..EngineProfile::default() },
                EngineProfile { id: "two".into(), name: "Second".into(), command: "/opt/katago gtp".into(), ..EngineProfile::default() },
            ],
            EngineStartup::Default,
        )
        .unwrap();
        wb.start_engines();
        let mut b = Bench { wb, world, held: Vec::new(), streams: HashSet::new(), _dir: dir };
        b.serve(&|_, _| None);
        b
    }

    /// The command as the adapter's `dispatch` runs it: the running mode's gate first, then the
    /// operation itself (only the operations these chains use).
    fn run(&mut self, command: Command) -> CommandResult {
        if let Some(result) = self.wb.game_gate(&command) {
            return result;
        }
        match command {
            Command::Game { op } => self.wb.game(op).unwrap(),
            Command::Match { op } => self.wb.engine_match(op).unwrap(),
            Command::HumanSl { op } => self.wb.human_sl(op).unwrap(),
            Command::Edit { op } => self.wb.edit(op).unwrap(),
            Command::ToggleAnalysis => {
                if self.wb.toggle_analysis().unwrap() { CommandResult::Done } else { CommandResult::Unchanged }
            }
            other => panic!("not used by these chains: {other:?}"),
        }
    }

    fn out(&mut self, id: InstanceId, text: &str) {
        self.wb.engine_input(id, EngineInput::Stdout(text.into()));
    }
    fn reply(&mut self, id: InstanceId, gtp: &str, text: &str) {
        let (sign, text) = text.strip_prefix('?').map_or(("=", text), |e| ("?", e));
        self.out(id, &format!("{sign}{gtp} {text}"));
        self.out(id, "");
    }

    /// Answers every GTP engine until quiet; `genmove` is always held, and `answer` may give a
    /// reply or "HOLD".
    fn serve(&mut self, answer: &dyn Fn(InstanceId, &str) -> Option<String>) -> Vec<(InstanceId, String)> {
        let mut seen = Vec::new();
        loop {
            let mut any = false;
            for id in self.world.gtp() {
                let pending = self.world.take(id);
                any |= !pending.is_empty();
                for line in pending {
                    let (gtp, cmd) = line.split_once(' ').unwrap();
                    seen.push((id, cmd.to_string()));
                    if self.streams.remove(&id) {
                        self.out(id, "");
                    }
                    if cmd.starts_with("genmove") {
                        self.held.push((id, gtp.into(), cmd.into()));
                        continue;
                    }
                    if let Some(text) = answer(id, cmd) {
                        if text == "HOLD" {
                            self.held.push((id, gtp.into(), cmd.into()));
                        } else {
                            self.reply(id, gtp, &text);
                        }
                        continue;
                    }
                    if cmd.starts_with("kata-analyze") {
                        self.out(id, &format!("={gtp}"));
                        self.streams.insert(id);
                        continue;
                    }
                    let text = match cmd.split(' ').next().unwrap() {
                        "name" => "KataGo",
                        "version" => "1.18.2",
                        "list_commands" => COMMANDS,
                        "kata-get-rules" => RULES,
                        "kata-get-param" => "1.5",
                        _ => "",
                    };
                    self.reply(id, gtp, text);
                }
            }
            if !any {
                return seen;
            }
        }
    }

    fn answer_held(&mut self, prefix: &str, text: &str) -> InstanceId {
        let at = self
            .held
            .iter()
            .position(|(_, _, c)| c.starts_with(prefix))
            .unwrap_or_else(|| panic!("no held {prefix}: {:?}", self.held));
        let (id, gtp, _) = self.held.remove(at);
        self.reply(id, &gtp, text);
        id
    }
    fn holds(&self, prefix: &str) -> bool {
        self.held.iter().any(|(_, _, c)| c.starts_with(prefix))
    }

    /// AI 陪练's analysis worker: answers its open queries with `vertex` (readiness included).
    fn worker_answer(&mut self, id: InstanceId, vertex: &str) {
        for _ in 0..6 {
            let pending = self.world.take(id);
            if pending.is_empty() {
                break;
            }
            for text in pending {
                let q: Value = serde_json::from_str(&text).unwrap();
                if q.get("action").is_some() {
                    continue;
                }
                let reply = json!({"id":q["id"],"humanPolicy":{(vertex):1.0},
                    "moveInfos":[{"move":vertex,"order":0,"utility":0.0,"visits":q["maxVisits"].as_u64().unwrap_or(1)}]});
                self.out(id, &reply.to_string());
            }
        }
        self.wb.engine_tick(Instant::now() + Duration::from_secs(5));
    }
    /// The open queries of AI 陪练's worker, unanswered.
    fn worker_queries(&mut self, id: InstanceId) -> Vec<Value> {
        self.world
            .take(id)
            .into_iter()
            .filter_map(|t| serde_json::from_str::<Value>(&t).ok())
            .filter(|q| q.get("action").is_none())
            .collect()
    }

    fn engine(&mut self) -> EngineView {
        self.wb.view(false).engine
    }
    fn game_phase(&mut self) -> Option<GamePhase> {
        self.wb.view(false).game.map(|g| g.phase)
    }
    fn match_phase(&mut self) -> Option<MatchPhase> {
        self.wb.view(false).engine_match.map(|m| m.phase)
    }
    fn stones(&self) -> usize {
        let rec = &self.wb.document().record;
        rec.path_to(self.wb.cursor()).iter().filter(|&&n| matches!(rec.node(n).kind, NodeKind::Move { .. })).count()
    }
}

fn human_game(black: bool) -> Command {
    Command::Game {
        op: GameCommand::Start {
            kind: GameKind::Genmove,
            settings: GameSettings { play_black: black, auto_save: false, ..GameSettings::default() },
            autoplay: None,
            engine: None,
        },
    }
}
fn engine_match() -> Command {
    Command::Match {
        op: MatchCommand::Start {
            settings: MatchSettings { rules: "chinese".into(), ..MatchSettings::default() },
            session: MatchSessionSettings {
                kind: GameKind::Genmove,
                auto_save: false,
                exchange_colors: false,
                ..MatchSessionSettings::default()
            },
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        },
    }
}
fn ai_coach(color: PlayerColor) -> Command {
    Command::HumanSl {
        op: HumanSlCommand::Start {
            settings: HumanSlTrainingSettings { player_color: color, ..HumanSlTrainingSettings::default() },
            from_current: false,
            advanced_visible: false,
        },
    }
}
fn play(point: Point) -> Command {
    Command::Edit { op: EditCommand::Play { point, color: None } }
}

// ---------------------------------------------------------------------------- chains

/// Review → engine match → 终止 with a move in flight → human game → review. The stopped
/// match holds both engines until each confirms its restore, so a game cannot start on an engine
/// still owed a move; that move, answered late, lands nowhere; the game afterwards binds only
/// its own moves; review gets the engines back.
#[test]
fn a_stopped_match_keeps_its_late_move_out_of_the_next_game_and_review() {
    let mut b = Bench::new();
    assert_eq!(b.engine().lease, LeaseAvailabilityView::Available);
    let review = b.wb.document().id;

    assert_eq!(b.run(engine_match()), CommandResult::Done);
    b.serve(&|_, _| None);
    assert_eq!(b.match_phase(), Some(MatchPhase::Playing));
    assert!(b.holds("genmove"), "the first mover is asked");
    let match_doc = b.wb.document().id;
    assert_ne!(match_doc, review, "the match plays on its own board");
    assert_eq!(b.engine().lease, LeaseAvailabilityView::PlayMode);

    assert_eq!(b.run(Command::Match { op: MatchCommand::Stop }), CommandResult::Done);
    // Both engines are given back behind the move still in flight: hold their fences.
    b.serve(&|_, c| (c == "name").then(|| "HOLD".into()));
    assert!(b.match_phase().is_some(), "not released before the engines confirm");
    assert!(
        matches!(b.run(human_game(true)), CommandResult::Refused { .. }),
        "a game cannot take an engine the stopped match still holds"
    );
    let (revision, cursor) = (b.wb.document().revision, b.wb.cursor());
    b.answer_held("genmove", "D4");
    assert_eq!((b.wb.document().revision, b.wb.cursor()), (revision, cursor), "the late match move lands nowhere");
    while b.holds("name") {
        b.answer_held("name", "KataGo");
    }
    b.serve(&|_, _| None);
    assert_eq!(b.match_phase(), None);
    assert_ne!(b.engine().lease, LeaseAvailabilityView::PlayMode);

    // The next mode: a human game on the engine the match gave back.
    assert_eq!(b.run(human_game(true)), CommandResult::Done);
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), Some(GamePhase::Playing));
    let game_doc = b.wb.document().id;
    assert_ne!(game_doc, match_doc);
    assert_eq!(b.stones(), 0, "nothing of the match is in the new game");
    assert_eq!(b.run(play(D4)), CommandResult::Done);
    b.serve(&|_, _| None);
    b.answer_held("genmove W", "Q16");
    b.serve(&|_, _| None);
    assert_eq!(b.stones(), 2);
    assert_eq!(b.run(Command::Game { op: GameCommand::End }), CommandResult::Done);
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), None);
    let engine = b.engine();
    assert_eq!(engine.lease, LeaseAvailabilityView::Available, "review has the foreground engine back");
    assert_eq!(engine.unrestored, None);
    assert_eq!(b.wb.document().id, game_doc, "review shows the game just played");
    assert_eq!(b.stones(), 2);
}

/// Review (paused by the user) → human game ended with a move in flight → AI 陪练 → 结束并复盘.
/// Each mode's own exit decides review's analysis, independently of the mode before it: an ended
/// game leaves analysis paused (Java `stopAiPlayingAndPolicy`), 实战后复盘 resumes it after a
/// successful exit (`HumanSlGameController.startPostGameAnalysisAfterSuccessfulExit`). The game's
/// late move lands neither in the game nor in AI 陪练's record.
#[test]
fn each_mode_exit_sets_review_analysis_and_late_moves_stay_out() {
    let mut b = Bench::new();
    assert_eq!(b.run(Command::ToggleAnalysis), CommandResult::Done);
    b.serve(&|_, _| None);
    assert!(b.engine().paused);

    assert_eq!(b.run(human_game(true)), CommandResult::Done);
    b.serve(&|_, _| None);
    b.run(play(D4));
    b.serve(&|_, _| None);
    assert!(b.holds("genmove W"));
    assert_eq!(b.run(Command::Game { op: GameCommand::End }), CommandResult::Done);
    b.serve(&|_, c| (c == "name").then(|| "HOLD".into()));
    assert_eq!(b.game_phase(), Some(GamePhase::Ending));
    assert!(
        matches!(b.run(ai_coach(PlayerColor::Black)), CommandResult::Refused { .. }),
        "AI 陪练 waits for the game's engine to be given back"
    );
    assert!(!b.world.0.lock().unwrap().iter().any(|p| p.analysis), "no worker was started");
    b.answer_held("genmove W", "Q16");
    b.answer_held("name", "KataGo");
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), None);
    assert_eq!(b.stones(), 1, "the late move is not in the ended game");
    assert!(b.engine().paused, "still the user's pause");

    let game_doc = b.wb.document().id;
    assert_eq!(b.run(ai_coach(PlayerColor::Black)), CommandResult::Done);
    let worker = b.world.worker();
    b.worker_answer(worker, "D4");
    b.serve(&|_, _| None);
    assert_eq!(b.wb.view(false).human_sl.map(|s| s.phase), Some(HumanSlPhase::Playing));
    assert_ne!(b.wb.document().id, game_doc, "AI 陪练 plays a new record");
    assert_eq!(b.stones(), 0);
    assert_eq!(b.run(play(D4)), CommandResult::Done);
    b.worker_answer(worker, "Q16");
    b.serve(&|_, _| None);
    assert_eq!(b.stones(), 2, "the human move and AI 陪练's answer");
    assert_eq!(b.run(Command::HumanSl { op: HumanSlCommand::Finish }), CommandResult::Done);
    b.serve(&|_, _| None);
    let view = b.wb.view(false);
    assert!(view.human_sl.is_none());
    assert!(b.world.dead(worker));
    assert_eq!(view.engine.lease, LeaseAvailabilityView::Available);
    assert!(!view.engine.paused, "实战后复盘 resumes analysis after the game");
    assert!(view.engine.analysing);
}

/// AI 陪练 with its AI thinking → 新对局 (deferred until AI 陪练 has exited) → the human game.
/// The AI answer arriving after the exit changes neither record, and the game's engine is the
/// foreground engine AI 陪练 handed back.
#[test]
fn a_game_requested_during_ai_coach_starts_after_its_exit_and_the_late_ai_move_lands_nowhere() {
    let mut b = Bench::new();
    b.serve(&|_, _| None);
    assert_eq!(b.run(ai_coach(PlayerColor::White)), CommandResult::Done);
    let worker = b.world.worker();
    let queries = b.worker_queries(worker);
    let ready = &queries[0];
    b.out(worker, &json!({"id":ready["id"],"humanPolicy":{"D4":1.0},"moveInfos":[{"move":"D4","order":0,"utility":0.0,"visits":1}]}).to_string());
    b.serve(&|_, _| None);
    assert!(b.wb.view(false).human_sl.is_some_and(|s| s.ai_thinking));
    let coach_doc = b.wb.document().id;
    let pending = b.worker_queries(worker);
    assert!(!pending.is_empty(), "AI 陪练 asked its worker for a move");

    // 新对局 during AI 陪练 is deferred to its exit.
    let new_game = human_game(true);
    b.run(new_game.clone());
    assert_eq!(b.game_phase(), None, "no game while AI 陪练 still holds the record");
    b.serve(&|_, _| None);
    assert!(b.wb.view(false).human_sl.is_none(), "AI 陪练 exited");
    assert!(b.world.dead(worker));
    let deferred = b.wb.take_human_sl_deferred().expect("the new game waits for the exit");
    assert_eq!(deferred, new_game);
    // The worker's answer arrives after the exit.
    for q in pending {
        b.out(worker, &json!({"id":q["id"],"humanPolicy":{"Q16":1.0},"moveInfos":[{"move":"Q16","order":0,"utility":0.0,"visits":q["maxVisits"].as_u64().unwrap_or(1)}]}).to_string());
    }
    b.wb.engine_tick(Instant::now() + Duration::from_secs(5));
    assert_eq!(b.wb.document().id, coach_doc);
    assert_eq!(b.stones(), 0, "the late AI move is not in AI 陪练's record");

    assert_eq!(b.run(deferred), CommandResult::Done);
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), Some(GamePhase::Playing));
    assert_eq!(b.engine().lease, LeaseAvailabilityView::PlayMode);
    assert_eq!(b.run(play(D4)), CommandResult::Done);
    b.serve(&|_, _| None);
    b.answer_held("genmove W", "Q16");
    assert_eq!(b.stones(), 2);
    let last = b.wb.document().record.node(b.wb.cursor()).kind;
    assert!(matches!(last, NodeKind::Move { color: Stone::White, point: Point { x: 15, y: 3 } }), "{last:?}");
    assert_eq!(b.wb.document().record.node(ROOT).children.len(), 1);
}

/// Human game → 死活题 during the game (refused like every other record edit) → 结束对局 →
/// 死活题 on the finished game. The frame's komi reaches the engine the game gave back, after
/// its restore; nothing of the frame happened while the game held the engine.
#[test]
fn tsumego_waits_for_the_game_and_its_komi_reaches_the_restored_engine() {
    let mut b = Bench::new();
    assert_eq!(b.run(human_game(true)), CommandResult::Done);
    b.serve(&|_, _| None);
    b.run(play(D4));
    b.serve(&|_, _| None);
    b.answer_held("genmove W", "Q16");
    b.serve(&|_, _| None);
    let frame = Command::Edit { op: EditCommand::FrameTsumego { settings: Default::default() } };
    let revision = b.wb.document().revision;
    assert!(matches!(b.run(frame.clone()), CommandResult::Refused { .. }), "a game record is not framed");
    assert_eq!(b.wb.document().revision, revision);

    assert_eq!(b.run(Command::Game { op: GameCommand::End }), CommandResult::Done);
    b.serve(&|_, c| (c == "name").then(|| "HOLD".into()));
    assert!(matches!(b.run(frame.clone()), CommandResult::Refused { .. }), "not before the engine is given back");
    b.answer_held("name", "KataGo");
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), None);

    assert!(matches!(b.run(frame), CommandResult::TsumegoFramed { .. }));
    let komi = b.wb.document().record.info.komi;
    assert!(komi != 7.5, "the frame sets its own komi: {komi}");
    let sent = b.serve(&|_, _| None);
    assert!(
        sent.iter().any(|(_, c)| *c == format!("komi {komi:.1}")),
        "the restored foreground engine gets the frame's komi: {sent:?}"
    );
    assert_eq!(b.engine().lease, LeaseAvailabilityView::Available);
}

/// AI 讲棋 asked on the review record → AI 陪练 replaces the record → the reply arrives. The
/// request belonged to the record left behind: it is cancelled with it and its late reply is
/// written nowhere, least of all into AI 陪练's record.
#[test]
fn a_commentary_reply_for_the_review_record_stays_out_of_ai_coach() {
    let mut b = Bench::new();
    let read = lizzie_core::formats::parse(
        lizzie_core::record::RecordFormat::Sgf,
        include_str!("fixtures/commentary/analyzed.sgf"),
        false,
    );
    let ticket = b.wb.begin_open(lizzie_core::workbench::OpenOrigin::Clipboard);
    assert!(b.wb.complete_open(ticket, read.map(|r| (lizzie_core::record::RecordFormat::Sgf, r))));
    b.serve(&|_, _| None);
    b.wb.set_commentary_connection(CommentaryProvider::ApiKey, "https://example.com".into(), "test-model".into(), false)
        .unwrap();
    b.wb.set_commentary_facts(CommentaryConnectionFacts { loaded: true, api_key_present: true, ..Default::default() });
    b.wb.commentary(CommentaryCommand::Open).unwrap();
    assert_eq!(
        b.wb.commentary(CommentaryCommand::Start { mode: CommentaryMode::Next, first: 1, last: 1000 }).unwrap(),
        CommandResult::Done
    );
    let job = b.wb.take_commentary_jobs().remove(0);

    assert_eq!(b.run(ai_coach(PlayerColor::Black)), CommandResult::Done);
    let worker = b.world.worker();
    b.worker_answer(worker, "D4");
    b.serve(&|_, _| None);
    assert_eq!(b.wb.view(false).human_sl.map(|s| s.phase), Some(HumanSlPhase::Playing));
    let coach = (b.wb.document().id, b.wb.document().revision);
    assert!(job.cancel.load(std::sync::atomic::Ordering::Acquire), "the request went with its record");
    assert!(!b.wb.commentary_finished(job.id, Ok("late lesson".into())));
    assert_eq!((b.wb.document().id, b.wb.document().revision), coach);
    assert!(b.wb.document().record.node(ROOT).comment.is_empty());
    assert!(b.wb.view(false).commentary.output.is_empty());
}

/// Review → 闪电分析 borrowing the foreground engine (a temporary analysis lease, not a mode
/// reservation) → every play/training mode is refused while it holds or returns the engine
/// (Java `beginEngineModeReservation` fails on an exclusive session) → cancel → after the confirmed
/// hand-back a human game takes the engine as a mode, and nothing of the flash run is in it.
#[test]
fn a_borrowed_flash_run_refuses_every_mode_until_its_engine_is_back() {
    let mut b = Bench::new();
    let reuse = lizzie_core::settings::AnalysisEngineSettings {
        command: "/fake/katago analysis -config /fake/analysis.cfg".into(),
        reuse_current_engine: true,
        ..Default::default()
    };
    b.wb.save_analysis_engine_settings(reuse, "", true).unwrap().unwrap();
    for point in [D4, Point { x: 15, y: 3 }, Point { x: 15, y: 15 }] {
        b.run(play(point));
    }
    b.serve(&|_, _| None);
    let review = (b.wb.document().id, b.wb.document().revision);
    assert_eq!(b.wb.start_flash_analysis(FlashTarget::WholeGame).unwrap(), Ok(true));
    let taken = b.serve(&|_, _| None);
    assert!(taken.iter().any(|(_, c)| c.contains("maxmoves")), "the flash run borrowed the engine: {taken:?}");
    assert_eq!(b.engine().lease, LeaseAvailabilityView::ExistingLease);

    for mode in [human_game(true), engine_match(), ai_coach(PlayerColor::Black)] {
        assert!(matches!(b.run(mode.clone()), CommandResult::Refused { .. }), "{mode:?} beside a borrowed engine");
    }
    assert!(!b.world.0.lock().unwrap().iter().any(|p| p.analysis), "no AI 陪练 worker");
    assert!(b.wb.cancel_flash_analysis());
    b.serve(&|_, c| (c == "name").then(|| "HOLD".into()));
    assert!(matches!(b.run(human_game(true)), CommandResult::Refused { .. }), "not before the hand-back");
    b.answer_held("name", "KataGo");
    b.serve(&|_, _| None);
    assert_eq!((b.wb.document().id, b.wb.document().revision), review, "the cancelled run wrote nothing");

    assert_eq!(b.run(human_game(true)), CommandResult::Done);
    b.serve(&|_, _| None);
    assert_eq!(b.game_phase(), Some(GamePhase::Playing));
    assert_eq!(b.engine().lease, LeaseAvailabilityView::PlayMode);
    assert!(b.wb.view(false).flash.is_none());
}

/// Review → Alt+, (one engine move, a mode reservation) still unanswered → AI 陪练 → the move
/// arrives late. It belongs to the review position; it lands neither in AI 陪练's record nor
/// anywhere once that record replaced the review one, and the engine's reservation is released.
#[test]
fn a_late_single_engine_move_stays_out_of_ai_coach() {
    let mut b = Bench::new();
    assert_eq!(b.run(Command::Game { op: GameCommand::EngineMove }), CommandResult::Done);
    b.serve(&|_, _| None);
    assert!(b.holds("genmove B"));
    let review = b.wb.document().id;

    // Java's Alt+, only sends `genmove` (LizzieFrame.genmove); it reserves nothing a new mode waits for.
    assert_eq!(b.run(ai_coach(PlayerColor::Black)), CommandResult::Done);
    let worker = b.world.worker();
    b.worker_answer(worker, "D4");
    b.serve(&|_, _| None);
    assert_eq!(b.wb.view(false).human_sl.map(|s| s.phase), Some(HumanSlPhase::Playing));
    let coach = (b.wb.document().id, b.wb.document().revision);
    assert_ne!(coach.0, review);
    b.answer_held("genmove B", "Q16");
    b.serve(&|_, _| None);
    assert_eq!((b.wb.document().id, b.wb.document().revision), coach, "the late move is not AI 陪练's");
    assert_eq!(b.stones(), 0);
    assert_eq!(b.engine().lease, LeaseAvailabilityView::HumanSlGame);
    assert_eq!(b.run(Command::HumanSl { op: HumanSlCommand::Finish }), CommandResult::Done);
    b.serve(&|_, _| None);
    let engine = b.engine();
    assert_eq!(engine.lease, LeaseAvailabilityView::Available, "the Alt+, reservation was released");
    assert_eq!(engine.unrestored, None);
}
