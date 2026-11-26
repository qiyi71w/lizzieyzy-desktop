//! Independent expectations: Java EngineGameBatchState, EngineGameModule and Leelaz at the
//! fixed migration baseline. Controlled processes exercise the same public Workbench interface.
use lizzie_core::api::*;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{CustomPdaMode, CustomPdaSettings, EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::record::{DocumentEdit, NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{MatchSettings, Storage};
use lizzie_core::workbench::Workbench;
use std::collections::HashSet;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Default)]
struct Process {
    id: InstanceId,
    sent: Vec<String>,
    /// Launched for a 自建算力 profile (`LaunchSpec.cloud`).
    cloud: bool,
}
#[derive(Clone, Default)]
struct World(Arc<Mutex<Vec<Process>>>);
impl World {
    fn take(&self, id: InstanceId) -> Vec<String> {
        std::mem::take(&mut self.0.lock().unwrap().iter_mut().find(|p| p.id == id).unwrap().sent)
    }
    fn ids(&self) -> Vec<InstanceId> {
        self.0.lock().unwrap().iter().map(|p| p.id).collect()
    }
}
struct Io(World, InstanceId);
impl EngineIo for Io {
    fn send(&mut self, line: &str) -> std::io::Result<()> {
        self.0
             .0
            .lock()
            .unwrap()
            .iter_mut()
            .find(|p| p.id == self.1)
            .unwrap()
            .sent
            .push(line.into());
        Ok(())
    }
    fn terminate(self: Box<Self>) {}
}
struct Launch(World);
impl Launcher for Launch {
    fn launch(&self, id: InstanceId, spec: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push(Process {
            id,
            cloud: spec.cloud.is_some(),
            ..Process::default()
        });
        Ok(Box::new(Io(self.0.clone(), id)))
    }
}
const COMMANDS:&str="name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nkata-analyze\nkata-get-rules\nkata-set-rules\nkata-get-param\nkata-set-param\ngenmove\nstop";
const RULES: &str = r#"{"friendlyPassOk":true,"hasButton":false,"ko":"SIMPLE","scoring":"AREA","suicide":false,"tax":"NONE","whiteHandicapBonus":"N"}"#;
struct Match {
    wb: Workbench,
    world: World,
    held: Vec<(InstanceId, String, String)>,
    streams: HashSet<InstanceId>,
    _dir: tempfile::TempDir,
}
impl Match {
    fn new() -> Self {
        Self::with_locale(None)
    }

    fn with_locale(locale: Option<&str>) -> Self {
        Self::with_profiles(
            locale,
            vec![
                EngineProfile {
                    id: "one".into(),
                    name: "First".into(),
                    command: "/opt/katago gtp".into(),
                    ..EngineProfile::default()
                },
                EngineProfile {
                    id: "two".into(),
                    name: "Second".into(),
                    command: "/opt/katago gtp".into(),
                    ..EngineProfile::default()
                },
            ],
        )
    }

    fn with_profiles(locale: Option<&str>, profiles: Vec<EngineProfile>) -> Self {
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            locale,
            Box::new(Launch(world.clone())),
            vec![],
        );
        wb.set_auto_quick_analyze(false).unwrap();
        wb.save_engine_settings(profiles, EngineStartup::Default).unwrap();
        Self {
            wb,
            world,
            held: Vec::new(),
            streams: HashSet::new(),
            _dir: dir,
        }
    }
    fn command(&mut self, op: MatchCommand) -> CommandResult {
        self.wb.engine_match(op).unwrap()
    }
    fn start(&mut self, values: MatchSessionSettings) {
        assert_eq!(
            self.command(MatchCommand::Start {
                settings: MatchSettings {
                    rules: "chinese".into(),
                    ..MatchSettings::default()
                },
                session: values,
                first_engine: 0,
                second_engine: 1,
                disable_wrn: true
            }),
            CommandResult::Done
        );
    }
    fn phase(&self) -> Option<MatchPhase> {
        self.wb.inspect().engine_match.map(|m| m.phase)
    }
    fn out(&mut self, instance: InstanceId, text: &str) {
        self.wb.engine_input(instance, EngineInput::Stdout(text.into()));
    }
    fn reply(&mut self, instance: InstanceId, id: &str, text: &str) {
        let (sign, text) = text.strip_prefix('?').map_or(("=", text), |s| ("?", s));
        self.out(instance, &format!("{sign}{id} {text}"));
        self.out(instance, "");
    }
    fn serve(&mut self, answer: &dyn Fn(InstanceId, &str) -> Option<String>) -> Vec<(InstanceId, String)> {
        let mut seen = Vec::new();
        loop {
            let mut any = false;
            for instance in self.world.ids() {
                let pending = self.world.take(instance);
                any |= !pending.is_empty();
                for line in pending {
                    let (id, cmd) = line.split_once(' ').unwrap();
                    seen.push((instance, cmd.into()));
                    if self.streams.remove(&instance) {
                        self.out(instance, "");
                    }
                    if cmd.starts_with("genmove") {
                        self.held.push((instance, id.into(), cmd.into()));
                        continue;
                    }
                    if let Some(text) = answer(instance, cmd) {
                        if text == "HOLD" {
                            self.held.push((instance, id.into(), cmd.into()));
                        } else {
                            self.reply(instance, id, &text);
                        }
                        continue;
                    }
                    let text = usual_answer(cmd);
                    if cmd.starts_with("kata-analyze") {
                        self.out(instance, &format!("={id}"));
                        self.streams.insert(instance);
                    } else {
                        self.reply(instance, id, text);
                    }
                }
            }
            if !any {
                return seen;
            }
        }
    }
    fn answer(&mut self, instance: InstanceId, prefix: &str, text: &str) {
        let at = self
            .held
            .iter()
            .position(|(i, _, cmd)| *i == instance && cmd.starts_with(prefix))
            .unwrap_or_else(|| panic!("no {instance}/{prefix}: {:?}", self.held));
        let (_, id, _) = self.held.remove(at);
        self.reply(instance, &id, text);
    }
    fn mover(&self) -> InstanceId {
        self.held
            .iter()
            .find(|(_, _, cmd)| cmd.starts_with("genmove"))
            .unwrap()
            .0
    }
    fn finish_turn(&mut self, text: &str) {
        let instance = self.mover();
        self.answer(instance, "genmove", text);
        self.serve(&|_, _| None);
    }
    fn successor(&mut self) {
        self.wb.engine_tick(Instant::now() + Duration::from_secs(1));
        self.serve(&|_, _| None);
    }
}
/// What a controlled participant answers when a test does not say otherwise.
fn usual_answer(command: &str) -> &'static str {
    match command.split(' ').next().unwrap() {
        "name" => "KataGo",
        "version" => "1.18.2",
        "list_commands" => COMMANDS,
        "kata-get-rules" => RULES,
        "kata-get-param" => "1.5",
        _ => "",
    }
}
fn genmove() -> MatchSessionSettings {
    MatchSessionSettings {
        kind: GameKind::Genmove,
        auto_save: false,
        exchange_colors: false,
        ..MatchSessionSettings::default()
    }
}
fn batch() -> MatchSessionSettings {
    MatchSessionSettings {
        batch_enabled: true,
        batch_limit: 2,
        exchange_colors: true,
        ..genmove()
    }
}

#[test]
fn acceptance_is_not_start_and_only_the_mover_is_asked() {
    let mut m = Match::new();
    m.start(genmove());
    assert_eq!(m.phase(), Some(MatchPhase::Starting));
    m.serve(&|_, c| (c == "time_settings 0 2 1").then_some("HOLD".into()));
    assert_eq!(m.phase(), Some(MatchPhase::Starting));
    assert!(!m.held.iter().any(|(_, _, c)| c.starts_with("genmove")));
    let ids = m.world.ids();
    for id in ids {
        m.answer(id, "time_settings", "");
    }
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    assert_eq!(m.held.iter().filter(|(_, _, c)| c.starts_with("genmove")).count(), 1);
    let first = m.mover();
    assert_eq!(m.wb.inspect().engine.foreground.unwrap().index, 0);
    m.finish_turn("D4");
    assert_ne!(m.mover(), first);
    assert_eq!(m.wb.inspect().engine.foreground.unwrap().index, 1);
    assert!(matches!(
        m.wb.document().record.node(m.wb.cursor()).kind,
        NodeKind::Move {
            color: Stone::Black,
            point: Point { x: 3, y: 15 }
        }
    ));
    m.finish_turn("Q16");
    assert_eq!(m.mover(), first);
    assert_eq!(m.wb.inspect().engine.foreground.unwrap().index, 0);
}

#[test]
fn stop_drops_late_answers_and_new_match_is_fenced() {
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    let mover = m.mover();
    let old_request = m
        .held
        .iter()
        .find(|(instance, _, command)| *instance == mover && command.starts_with("genmove"))
        .unwrap()
        .1
        .clone();
    m.command(MatchCommand::Stop);
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    let before = (m.wb.document().id, m.wb.cursor(), m.wb.document().revision);
    m.serve(&|_, c| (c == "name").then_some("HOLD".into()));
    m.answer(mover, "genmove", "D4");
    assert_eq!((m.wb.document().id, m.wb.cursor(), m.wb.document().revision), before);
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    let seats = m
        .held
        .iter()
        .filter(|(_, _, c)| c == "name")
        .map(|(i, _, _)| *i)
        .collect::<Vec<_>>();
    m.answer(seats[0], "name", "KataGo");
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    m.answer(seats[1], "name", "KataGo");
    assert_eq!(m.phase(), None);
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.end, MatchEnd::UserStopped);
    assert!(outcome.unrestored.is_empty());
    m.start(genmove());
    m.serve(&|_, _| None);
    assert_ne!(m.wb.document().id, before.0);
    assert_eq!(m.wb.cursor(), ROOT);
    let new_document = (m.wb.document().id, m.wb.cursor(), m.wb.document().revision);
    m.reply(mover, &old_request, "Q16");
    assert_eq!(
        (m.wb.document().id, m.wb.cursor(), m.wb.document().revision),
        new_document,
        "an old reservation/request cannot commit into the new game"
    );
}

#[test]
fn move_binding_rejects_a_changed_node_stamp() {
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    let mover = m.mover();
    m.wb.edit_document(DocumentEdit::SetupStone {
        point: Point { x: 0, y: 0 },
        stone: Some(Stone::Black),
    })
    .unwrap();
    let revision = m.wb.document().revision;
    m.answer(mover, "genmove", "D4");
    assert_eq!(m.wb.document().revision, revision);
    assert_eq!(m.wb.cursor(), ROOT);
}

#[test]
fn stop_in_starting_and_between_games_never_starts_a_successor() {
    let mut m = Match::new();
    m.start(batch());
    m.command(MatchCommand::Stop);
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), None);
    assert_eq!(m.wb.cursor(), ROOT);
    m.start(batch());
    m.serve(&|_, _| None);
    m.finish_turn("pass");
    m.finish_turn("pass");
    assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
    let doc = m.wb.document().id;
    m.command(MatchCommand::Stop);
    m.successor();
    assert_eq!(m.phase(), None);
    assert_eq!(m.wb.document().id, doc);
}

/// The open document's engine-game score side: `None` when it is no engine game.
fn pk(m: &Match) -> Option<Option<Stone>> {
    m.wb.inspect().pk_board.map(|p| p.score_side)
}

fn open_sgf(m: &mut Match, sgf: &str) {
    let record = lizzie_core::formats::sgf::read(sgf).unwrap();
    let t = m.wb.begin_open(lizzie_core::workbench::OpenOrigin::File { path: "/x.sgf".into(), source: OpenSource::Drop });
    assert!(m.wb.complete_open(t, Ok((lizzie_core::record::RecordFormat::Sgf, record))));
}

#[test]
fn an_engine_game_board_stays_one_until_replaced_and_a_saved_dz_makes_one() {
    let open = open_sgf;
    let mut m = Match::new();
    assert_eq!(pk(&m), None);
    // A start aborted before any move leaves no engine game (EngineManager.java:5310).
    m.start(genmove());
    assert_eq!(pk(&m), Some(None), "both engines are still unnamed: no score side yet");
    m.command(MatchCommand::Stop);
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), None);
    assert_eq!(pk(&m), None);
    // A played game: the white KataGo engine gives the score, during and after the match.
    m.start(genmove());
    m.serve(&|_, _| None);
    assert_eq!(pk(&m), Some(Some(Stone::White)));
    m.finish_turn("resign");
    assert_eq!(m.phase(), None);
    assert_eq!(pk(&m), Some(Some(Stone::White)));
    // A new board is no engine game.
    let new_board = |confirmed| EditCommand::NewBoard { confirmed };
    if let CommandResult::NeedsEditConfirmation { target } = m.wb.edit(new_board(None)).unwrap() {
        assert_eq!(m.wb.edit(new_board(Some(target))).unwrap(), CommandResult::Done);
    }
    assert_eq!(pk(&m), None);
    // A saved engine game (SGFParser.java:775-791), then a plain record.
    open(&mut m, "(;GM[1]SZ[19]DZ[KW];B[pd];W[dd])");
    assert_eq!(pk(&m), Some(Some(Stone::White)));
    open(&mut m, "(;GM[1]SZ[19]DZ[KB])");
    assert_eq!(pk(&m), Some(Some(Stone::Black)));
    open(&mut m, "(;GM[1]SZ[19]DZ[Y])");
    assert_eq!(pk(&m), Some(None));
    open(&mut m, "(;GM[1]SZ[19];B[pd])");
    assert_eq!(pk(&m), None);
}

#[test]
fn a_saved_engine_game_reopens_as_one_until_cleared_and_converting_rereads_its_dz() {
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    m.finish_turn("D4");
    m.finish_turn("resign");
    assert_eq!(m.phase(), None);
    // A user save of the finished game writes its DZ (SGFParser.java:1443-1462), once.
    let job = m.wb.begin_save("/games/saved.sgf", "test");
    assert!(job.text.contains("DZ[KW]"), "{}", job.text);
    assert_eq!(job.text.matches("DZ[").count(), 1);
    assert!(m.wb.complete_save(job.ticket, Ok(())));
    open_sgf(&mut m, &job.text);
    assert_eq!(pk(&m), Some(Some(Stone::White)));
    // 清除Lizzie缓存: this move's keeps the marks, the global one ends them (Board.java:5410).
    m.wb.clear_analysis_cache(AnalysisScope::Current);
    assert_eq!(pk(&m), Some(Some(Stone::White)));
    m.wb.clear_analysis_cache(AnalysisScope::All);
    assert_eq!(pk(&m), None);
    // 转换为初始局面 reads them again from the root DZ the record keeps (Board.java:1320).
    m.wb.navigate(lizzie_core::browse::Navigation::Last);
    let convert = |confirmed| EditCommand::ConvertToStartingPosition { confirmed };
    if let CommandResult::NeedsEditConfirmation { target } = m.wb.edit(convert(None)).unwrap() {
        assert_eq!(m.wb.edit(convert(Some(target))).unwrap(), CommandResult::Done);
    }
    assert!(!m.wb.document().record.has_actions());
    assert_eq!(pk(&m), Some(Some(Stone::White)));
}

#[test]
fn double_pass_and_move_limit_leave_re_empty_and_do_not_score() {
    // Leelaz.java:5880 checks moveNumber > maxGameMoves before committing the answered move.
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    m.finish_turn("pass");
    m.finish_turn("pass");
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.end, MatchEnd::DoublePass);
    assert_eq!(outcome.score.double_passes, 1);
    assert_eq!(outcome.winner, None);
    assert!(m.wb.document().record.info.result.is_empty());
    m.start(MatchSessionSettings {
        max_moves_enabled: true,
        max_moves: 1,
        ..genmove()
    });
    m.serve(&|_, _| None);
    m.finish_turn("D4");
    m.finish_turn("Q16");
    m.finish_turn("C3");
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.end, MatchEnd::MaxMoves);
    assert_eq!(outcome.score.max_moves, 1);
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 2);
    assert!(m.wb.document().record.info.result.is_empty());
}

#[test]
fn resign_scores_participant_color_and_exchanges_after_both_releases() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    let first = m.mover();
    m.answer(first, "genmove", "resign");
    m.serve(&|_, c| (c == "name").then_some("HOLD".into()));
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    let doc = m.wb.document().id;
    m.successor();
    assert_eq!(m.wb.document().id, doc);
    let seats = m
        .held
        .iter()
        .filter(|(_, _, c)| c == "name")
        .map(|(i, _, _)| *i)
        .collect::<Vec<_>>();
    m.answer(seats[0], "name", "KataGo");
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    m.answer(seats[1], "name", "KataGo");
    assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
    let first_result = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(first_result.result, "W+R");
    assert_eq!(first_result.score.second_white_wins, 1);
    m.successor();
    assert_ne!(m.mover(), first);
    let view = m.wb.inspect().engine_match.unwrap();
    assert_eq!(view.ordinal, 2);
    assert!(!view.first_is_black);
    m.finish_turn("resign");
    let result = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(result.result, "W+R");
    assert_eq!(result.score.first_white_wins, 1);
    assert_eq!(result.score.second_white_wins, 1);
    assert!(result.batch_finished);
    assert_eq!(m.phase(), None);
}

#[test]
fn pause_commits_the_inflight_answer_but_dispatches_nothing_until_settled_resume() {
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    m.command(MatchCommand::Pause);
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    assert_eq!(
        m.command(MatchCommand::Pause),
        CommandResult::Refused {
            message_key: "BottomToolbar.genmoveStopHint".into()
        }
    );
    let mover = m.mover();
    m.answer(mover, "genmove", "D4");
    let settled = m.serve(&|_, _| None);
    for participant in m.world.ids() {
        assert!(
            settled
                .iter()
                .any(|(instance, command)| *instance == participant && command == "stop"),
            "both participants stop after recording the pending next turn"
        );
    }
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 1);
    assert!(!m.held.iter().any(|(_, _, c)| c.starts_with("genmove")));
    m.command(MatchCommand::Pause);
    m.serve(&|_, _| None);
    assert!(m.held.iter().any(|(_, _, c)| c == "genmove W"));
}

#[test]
fn participant_exit_retires_and_restore_refusal_stops_batch() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    let id = m.mover();
    m.wb.engine_input(id, EngineInput::Exited(Some(1)));
    m.serve(&|_, _| None);
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.end, MatchEnd::Retired);
    assert!(outcome.batch_finished);
    assert_eq!(m.phase(), None);
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    let id = m.mover();
    m.answer(id, "genmove", "resign");
    m.serve(&|_, c| c.starts_with("kata-set-rules").then_some("?restore refused".into()));
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(
        outcome.unrestored,
        vec![UnrestoredReason::Restore, UnrestoredReason::Restore]
    );
    assert!(outcome.batch_finished);
    assert_eq!(m.phase(), None);
}

/// EngineManager.java:5403-5506, 8845-9093: the participant that ended is started again once the
/// match is retired, in the foreground seat it held and synced to the position then current.
#[test]
fn a_participant_that_ended_starts_again_after_the_match_is_retired() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    let id = m.mover();
    let index = m.wb.view(false).engine.foreground.unwrap().index;
    let before = m.world.ids().len();
    m.wb.engine_input(id, EngineInput::Exited(Some(1)));
    let restart = m.serve(&|_, _| None);
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!((outcome.end, outcome.batch_finished), (MatchEnd::Retired, true));
    assert_eq!(m.phase(), None);
    let ids = m.world.ids();
    assert_eq!(ids.len(), before + 1, "only the ended participant starts again");
    let new = *ids.last().unwrap();
    let v = m.wb.view(false);
    let fg = v.engine.foreground.expect("back in its seat");
    assert_eq!(fg.index, index);
    assert!(fg.ready);
    let moves = m.wb.document().record.node(m.wb.cursor()).move_number;
    let plays = restart.iter().filter(|(i, c)| *i == new && c.starts_with("play ")).count();
    assert_eq!(plays as u32, moves, "synced to the position after the match");
}

/// Leelaz.java:10419-10434: a participant's end handled by the match is not shown as an engine
/// failure; a replacement that cannot start is marked not loaded without the ordinary start-up
/// diagnostic either, and is not tried again.
#[test]
fn a_participant_replacement_that_fails_to_start_is_quiet_and_not_retried() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    let id = m.mover();
    let newest = *m.world.ids().iter().max().unwrap();
    m.wb.engine_input(id, EngineInput::Exited(Some(1)));
    assert!(m.wb.view(false).engine_failures.is_empty(), "the participant's end is not shown");
    m.serve(&move |i, c| (i > newest && c == "name").then(|| "?cannot start".into()));
    let v = m.wb.view(false);
    assert!(v.engine_failures.is_empty(), "no start-up diagnostic for the replacement");
    assert!(v.engine.foreground.is_none() && v.engine.down);
    let count = m.world.ids().len();
    m.wb.engine_tick(Instant::now() + Duration::from_secs(60));
    m.serve(&|_, _| None);
    assert_eq!(m.world.ids().len(), count, "not tried again");
}

#[test]
fn live_limit_and_intervention_resign_follow_java_batch_rules() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    m.command(MatchCommand::ReviseBatchLimit { limit: 1 });
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::Resign { side: Stone::White },
    });
    m.serve(&|_, _| None);
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.result, "B+R");
    assert_eq!(outcome.score.first_black_wins, 1);
    assert!(outcome.batch_finished);
    assert_eq!(m.phase(), None);
}

#[test]
fn analysis_resignation_is_per_side_and_only_counts_completed_turns() {
    let mut m = Match::new();
    m.start(MatchSessionSettings {
        kind: GameKind::Analysis,
        time_enabled: false,
        visits_enabled: true,
        first_visits: 10,
        second_visits: 10,
        auto_save: false,
        ..MatchSessionSettings::default()
    });
    m.serve(&|_, _| None);
    // Java Leelaz 8863-8888: moveNumber > minMove, consecutive low-winrate turns per side.
    for (mv, wr) in [("D4", 50), ("Q16", 5), ("C3", 50), ("R17", 5)] {
        let idx = m.wb.inspect().engine_match.as_ref().unwrap().thinking_engine.unwrap();
        let id = m.world.ids()[idx as usize];
        let probability = f64::from(wr) / 100.0;
        m.out(
            id,
            &format!("info move {mv} visits 10 winrate {probability} prior 0.5 order 0 pv {mv}"),
        );
        m.serve(&|_, _| None);
    }
    let outcome = m.wb.view(false).match_outcome.unwrap();
    assert_eq!(outcome.end, MatchEnd::Resigned);
    assert_eq!(outcome.result, "B+R");
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 3);
}

#[test]
fn winrate_confirmation_fences_successor_and_opening_is_replayed() {
    let mut m = Match::new();
    m.command(MatchCommand::ReplaceOpenings {
        files: vec![MatchOpeningFile {
            path: "opening.sgf".into(),
            bytes: b"(;SZ[19];B[dd];W[pp])".to_vec(),
        }],
    });
    let values = MatchSessionSettings {
        save_winrate: true,
        ..batch()
    };
    m.command(MatchCommand::Start {
        settings: MatchSettings {
            sgf_start: true,
            sgf_random: false,
            ..MatchSettings::default()
        },
        session: values,
        first_engine: 0,
        second_engine: 1,
        disable_wrn: true,
    });
    m.serve(&|_, _| None);
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 2);
    m.finish_turn("resign");
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    let view = m.wb.view(false);
    let request = view.match_winrate_image.unwrap();
    assert!(request.path.ends_with(".png"));
    m.successor();
    assert_eq!(m.phase(), Some(MatchPhase::Ending));
    m.command(MatchCommand::WinrateImageDone {
        game: request.game,
        error: None,
    });
    assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
    m.successor();
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 2);
}

/// A capture, then stones set up mid-game with a side to play that does not alternate.
const MID_GAME_SETUP: &str = "(;SZ[19];B[ba];W[aa];B[ab];AB[dd]PL[B])";

/// The stones, captures and side to play of `node`.
fn position_at(rec: &lizzie_core::record::GameRecord, node: lizzie_core::record::NodeId) -> (Vec<Option<Stone>>, u32, u32, Stone) {
    let pos = rec.position(node);
    (pos.stones, pos.black_captures, pos.white_captures, rec.node(node).to_play)
}

#[test]
fn a_match_opening_at_a_mid_game_setup_keeps_its_stones_and_side_to_play_for_the_engines_and_the_file() {
    let source = lizzie_core::formats::sgf::read(MID_GAME_SETUP).unwrap();
    let expected = position_at(&source, source.line_end(ROOT));
    assert_eq!((expected.1, expected.3), (1, Stone::Black), "the sample captures one stone and gives Black the move");
    // SGF 开局 from a file, and 固定当前局面 at the last node of the open record.
    for from_file in [true, false] {
        let mut m = Match::new();
        let session = if from_file {
            m.command(MatchCommand::ReplaceOpenings {
                files: vec![MatchOpeningFile { path: "setup.sgf".into(), bytes: MID_GAME_SETUP.as_bytes().to_vec() }],
            });
            genmove()
        } else {
            let t = m.wb.begin_open(lizzie_core::workbench::OpenOrigin::File { path: "/x.sgf".into(), source: OpenSource::Drop });
            assert!(m.wb.complete_open(t, Ok((lizzie_core::record::RecordFormat::Sgf, source.clone()))));
            m.wb.navigate(lizzie_core::browse::Navigation::Last);
            MatchSessionSettings { continue_position: true, ..genmove() }
        };
        m.command(MatchCommand::Start {
            settings: MatchSettings { rules: "chinese".into(), sgf_start: from_file, sgf_random: false, ..MatchSettings::default() },
            session,
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        });
        let sent = m.serve(&|_, _| None);
        let rec = m.wb.document().record.clone();
        assert_eq!(position_at(&rec, m.wb.cursor()), expected, "from_file {from_file}: the match starts where the node stood");
        let black = m.mover();
        let to_black: Vec<&str> = sent.iter().filter(|(i, _)| *i == black).map(|(_, c)| c.as_str()).collect();
        assert!(to_black.contains(&"play B D16"), "from_file {from_file}: {to_black:?}");
        assert!(!to_black.contains(&"play W A19"), "from_file {from_file}: the position is set, not replayed: {to_black:?}");
        assert!(m.held.iter().any(|(i, _, c)| *i == black && c == "genmove B"), "from_file {from_file}: {:?}", m.held);
        let path = m._dir.path().join("match.sgf");
        let job = m.wb.begin_save(path.to_str().unwrap(), "LizzieYzy Tauri:test");
        let reopened = lizzie_core::formats::sgf::read(&job.text).unwrap();
        assert_eq!(position_at(&reopened, reopened.line_end(ROOT)), expected, "from_file {from_file}: {}", job.text);
    }
}

#[test]
fn unverifiable_rules_require_consent_and_verified_mismatch_is_not_consent() {
    let mut m = Match::new();
    m.start(genmove());
    let plain = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\ngenmove";
    m.serve(&|_, command| (command == "list_commands").then(|| plain.to_string()));
    let view = m.wb.inspect().engine_match.unwrap();
    assert_eq!(view.phase, MatchPhase::Starting);
    assert!(view.rules_consent_required);
    assert_eq!(view.rules_unverified.len(), 2);
    assert!(view
        .rules_unverified
        .iter()
        .all(|p| p.reason_key == "MatchRules.unconfirmed"));
    assert!(!m.held.iter().any(|(_, _, c)| c.starts_with("genmove")));
    m.command(MatchCommand::ConfirmRules {
        game: view.game,
        allow: true,
    });
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));

    let mut m = Match::new();
    m.command(MatchCommand::Start {
        settings: MatchSettings {
            rules: "japanese".into(),
            ..MatchSettings::default()
        },
        session: genmove(),
        first_engine: 0,
        second_engine: 1,
        disable_wrn: true,
    });
    // Both engines claim to accept the overlay but still report Chinese rules.
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), None);
    assert_eq!(m.wb.view(false).match_outcome.unwrap().end, MatchEnd::Retired);
    assert!(!m.held.iter().any(|(_, _, c)| c.starts_with("genmove")));
}

/// 本局规则 is named by the core's rules presets: Japanese rules saved by earlier versions with
/// friendlyPassOk true are still 日本/韩国规则 (territory scoring does not use that field), while a
/// real difference is 自定义.
#[test]
fn territory_match_rules_keep_their_name_whatever_friendly_pass_says() {
    let japanese = |friendly: bool, tax: &str| {
        format!(
            r#"{{"ko":"SIMPLE","scoring":"TERRITORY","tax":"{tax}","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":{friendly}}}"#
        )
    };
    for (rules, choice) in [
        (japanese(true, "SEKI"), Some("japanese")),
        (japanese(false, "SEKI"), Some("japanese")),
        (japanese(true, "NONE"), None),
    ] {
        let mut m = Match::new();
        m.command(MatchCommand::Start {
            settings: MatchSettings {
                rules,
                ..MatchSettings::default()
            },
            session: genmove(),
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        });
        let view = m.wb.inspect().engine_match.unwrap();
        assert_eq!(view.rules_target_choice.as_deref(), choice);
    }
}

#[test]
fn spec_03_komi_revision_commits_only_after_both_participants_confirm() {
    // EngineManager.java:7624–7756: applied KM stays old until both confirmations;
    // a refusal retains the old value, pauses and retires the captured participants.
    for refused in [false, true] {
        let mut m = Match::new();
        m.start(batch());
        m.serve(&|_, _| None);
        assert!(m.wb.set_komi(6.5).unwrap());
        assert_eq!(m.wb.document().record.info.komi, 7.5);
        let mover = m.mover();
        m.answer(mover, "genmove", "D4");
        m.serve(&|_, command| (command == "komi 6.5").then_some("HOLD".into()));
        let before_revision = m.wb.document().revision;
        assert_eq!(m.wb.document().record.info.komi, 7.5, "both confirmations are pending");
        assert!(!m.held.iter().any(|(_, _, command)| command.starts_with("genmove")));
        let seats = m.world.ids();
        m.answer(seats[0], "komi 6.5", "");
        assert_eq!(
            m.wb.document().record.info.komi,
            7.5,
            "one confirmation is not a commit"
        );
        assert_eq!(m.wb.document().revision, before_revision);
        m.answer(seats[1], "komi 6.5", if refused { "?cannot apply komi" } else { "" });
        m.serve(&|_, _| None);
        if refused {
            assert_eq!(m.wb.document().record.info.komi, 7.5);
            let view = m.wb.view(false);
            let outcome = view.match_outcome.expect("failed komi update is reported");
            assert_eq!(outcome.end, MatchEnd::Retired);
            assert!(outcome.batch_finished && view.engine.paused);
            assert_eq!(m.phase(), None);
        } else {
            assert_eq!(m.wb.document().record.info.komi, 6.5);
            assert_eq!(m.wb.document().revision, before_revision + 1, "KM commits exactly once");
            assert!(m.held.iter().any(|(_, _, command)| command == "genmove W"));
            assert_eq!(m.phase(), Some(MatchPhase::Playing));
        }
    }
}

/// The Lizzie cache does not hold a stored result against the playing engine's search
/// (`BoardData`: `!Lizzie.engineGame.current().playing()`): each new result is shown.
#[test]
fn an_engine_game_shows_each_new_result_even_when_weaker() {
    let mut m = Match::new();
    m.start(MatchSessionSettings {
        kind: GameKind::Analysis,
        time_enabled: false,
        visits_enabled: true,
        first_visits: 1000,
        second_visits: 1000,
        auto_save: false,
        ..MatchSessionSettings::default()
    });
    m.serve(&|_, _| None);
    let first = m.world.ids()[0];
    m.out(first, "info move D4 visits 10 winrate 0.5 prior 0.5 order 0 pv D4");
    m.out(first, "info move Q16 visits 5 winrate 0.5 prior 0.5 order 0 pv Q16");
    let analysis = m.wb.inspect().analysis.unwrap();
    assert_eq!((analysis.node, analysis.visits), (ROOT, 5));
}

/// EngineManager.java:2607-2617: an analysis-mode engine game with the Leela window's Visits
/// 自动加载 lifts each participant's visit limit before `clear_cache`. Only a Leela Zero / Sai
/// participant gets it; Java sends it to KataGo too, which refuses it.
#[test]
fn an_analysis_match_lifts_the_visit_limit_of_leela_participants_only() {
    const LEELA: &str = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nlz-analyze\nlz-setoption\nclear_cache\ngenmove";
    let mut m = Match::new();
    m.wb.start_engines();
    let leela = m.world.ids()[0];
    let as_leela = move |instance: InstanceId, command: &str| -> Option<String> {
        if instance != leela {
            return None;
        }
        match command {
            "name" => Some("Leela Zero".into()),
            "version" => Some("0.17".into()),
            "list_commands" => Some(LEELA.into()),
            _ => None,
        }
    };
    m.serve(&as_leela);
    let visits = lizzie_core::engine::profile::LeelaOptionSetting { enabled: true, auto_load: true, value: "50".into() };
    let options = lizzie_core::engine::profile::LeelaOptionSettings { visits, ..Default::default() };
    assert_eq!(m.wb.save_leela_engine_options(options).unwrap(), None);
    m.serve(&as_leela);
    m.start(MatchSessionSettings {
        kind: GameKind::Analysis,
        time_enabled: false,
        visits_enabled: true,
        first_visits: 1000,
        second_visits: 1000,
        auto_save: false,
        ..MatchSessionSettings::default()
    });
    let sent = m.serve(&as_leela);
    let to = |id: InstanceId| sent.iter().filter(move |(i, _)| *i == id).map(|(_, c)| c.as_str()).collect::<Vec<_>>();
    let leela_sent = to(leela);
    let lift = leela_sent.iter().position(|c| *c == "lz-setoption name Visits value 1000000000").expect("lifted");
    let clear = leela_sent.iter().position(|c| *c == "clear_cache").expect("clear_cache");
    assert!(lift < clear, "{leela_sent:?}");
    for other in m.world.ids().into_iter().filter(|&i| i != leela) {
        assert!(!to(other).iter().any(|c| c.starts_with("lz-setoption")), "KataGo gets no lz-setoption: {:?}", to(other));
    }
}

/// The lifted visit limit is the match's own: when the match ends, the same Leela Zero / Sai
/// process gets back the Visits value it had before, confirmed before it is handed back.
#[test]
fn an_analysis_match_gives_the_leela_visit_limit_back_when_it_ends() {
    const LEELA: &str = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\nlz-analyze\nlz-setoption\nclear_cache\ngenmove";
    let mut m = Match::new();
    m.wb.start_engines();
    let leela = m.world.ids()[0];
    let as_leela = move |instance: InstanceId, command: &str| -> Option<String> {
        if instance != leela {
            return None;
        }
        match command {
            "name" => Some("Leela Zero".into()),
            "version" => Some("0.17".into()),
            "list_commands" => Some(LEELA.into()),
            _ => None,
        }
    };
    m.serve(&as_leela);
    let visits = lizzie_core::engine::profile::LeelaOptionSetting { enabled: true, auto_load: true, value: "800".into() };
    let options = lizzie_core::engine::profile::LeelaOptionSettings { visits, ..Default::default() };
    assert_eq!(m.wb.save_leela_engine_options(options).unwrap(), None);
    m.serve(&as_leela);
    m.start(MatchSessionSettings {
        kind: GameKind::Analysis,
        time_enabled: false,
        visits_enabled: true,
        first_visits: 1000,
        second_visits: 1000,
        auto_save: false,
        ..MatchSessionSettings::default()
    });
    let mut sent = m.serve(&as_leela);
    m.command(MatchCommand::Stop);
    sent.extend(m.serve(&as_leela));
    let leela_sent: Vec<&str> = sent.iter().filter(|(i, _)| *i == leela).map(|(_, c)| c.as_str()).collect();
    let lift = leela_sent.iter().position(|c| *c == "lz-setoption name Visits value 1000000000").expect("lifted");
    let restore = leela_sent.iter().rposition(|c| *c == "lz-setoption name Visits value 800");
    assert!(restore.is_some_and(|at| at > lift), "{leela_sent:?}");
    assert_eq!(m.phase(), None);
    assert!(m.wb.view(false).match_outcome.unwrap().unrestored.is_empty());
    for other in m.world.ids().into_iter().filter(|&i| i != leela) {
        let to_other: Vec<_> = sent.iter().filter(|(i, _)| *i == other).map(|(_, c)| c.as_str()).collect();
        assert!(!to_other.iter().any(|c| c.starts_with("lz-setoption")), "KataGo gets no lz-setoption: {to_other:?}");
    }
}

#[test]
fn analysis_intervention_uses_current_candidates_while_previous_candidates_are_shown() {
    let mut m = Match::new();
    let values = MatchSessionSettings {
        kind: GameKind::Analysis,
        time_enabled: false,
        visits_enabled: true,
        first_visits: 1000,
        second_visits: 1000,
        random_moves_enabled: true,
        random_winrate_diff: 1.0,
        auto_save: false,
        ..MatchSessionSettings::default()
    };
    let settings = MatchSettings {
        rules: "chinese".into(),
        show_previous_bestmoves: true,
        previous_bestmove_only: true,
        check_random_visits: true,
        random_visits_percent: 25.0,
        ..MatchSettings::default()
    };
    m.command(MatchCommand::Start {
        settings,
        session: values,
        first_engine: 0,
        second_engine: 1,
        disable_wrn: true,
    });
    m.serve(&|_, _| None);
    let first = m.world.ids()[0];
    m.out(first,"info move D4 visits 10 winrate 0.5 scoreLead 0.5 scoreStdev 1.0 prior 0.5 order 0 pv D4 info move Q16 visits 1 winrate 0.1 prior 0.1 order 1 pv Q16");
    assert_eq!(m.wb.cursor(), ROOT);
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::PlayNow,
    });
    m.serve(&|_, _| None);
    assert!(matches!(
        m.wb.document().record.node(m.wb.cursor()).kind,
        NodeKind::Move {
            point: Point { x: 3, y: 15 },
            color: Stone::Black
        }
    ));
    let view = m.wb.inspect();
    let analysis = view.analysis.unwrap();
    assert_eq!(analysis.node, ROOT);
    assert_eq!(analysis.candidates.len(), 1);
    assert!(
        !m.wb.document().record.node(ROOT).comment.is_empty(),
        "Java PK comments are live document edits"
    );
    assert!(
        m.wb.document().record.node(ROOT).comment.contains("1.5"),
        "the captured nonzero PDA is included in the PK comment; disabled WRN is zero"
    );
    assert!(!m.wb.navigate(lizzie_core::browse::Navigation::First));
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::ToggleManual,
    });
    m.wb.edit(EditCommand::Play {
        point: Point { x: 15, y: 3 },
        color: None,
    })
    .unwrap();
    let sent = m.serve(&|_, _| None);
    assert_eq!(m.wb.inspect().engine_match.unwrap().thinking_engine, Some(0));
    assert!(sent
        .iter()
        .filter(|(_, c)| c.starts_with("kata-analyze"))
        .all(|(instance, _)| *instance == first));
}

#[test]
fn batch_records_and_summary_keep_java_result_metadata_and_confidence() {
    let mut m = Match::new();
    m.start(batch());
    m.serve(&|_, _| None);
    m.finish_turn("resign");
    let first = m.wb.take_background_saves();
    let sgf = first
        .iter()
        .find(|j| j.path.extension().is_some_and(|e| e == "sgf"))
        .unwrap();
    assert!(sgf.text.contains("DZ[KW]"));
    assert!(sgf.text.contains("RE[W+R]"));
    assert!(
        sgf.text.contains("PB[First]") && sgf.text.contains("PW[Second]") && sgf.text.contains("AP[LizzieYzy Tauri]")
    );
    m.successor();
    m.finish_turn("resign");
    let jobs = m.wb.take_background_saves();
    let summary = jobs
        .iter()
        .find(|j| j.path.extension().is_some_and(|e| e == "txt"))
        .unwrap();
    // Java writeToFile: two wins split 1:1, Wilson 2-sigma at n=2 = 50 ± 40.82%.
    assert!(summary.text.contains("50.00% ± 40.82%"));
    assert!(summary.text.contains("1:1"));
    assert!(summary.text.contains("\r\n\r\n"));
}

#[test]
fn a_batch_folder_must_not_resolve_to_the_parent_directory() {
    for folder in [".", "..", "/", "\\", "///", ".../..."] {
        let mut m = Match::new();
        let result = m.wb.engine_match(MatchCommand::Start {
            settings: MatchSettings::default(),
            session: MatchSessionSettings {
                batch_name: folder.into(),
                ..batch()
            },
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        });
        assert!(result.is_err(), "{folder}");
        assert!(m.world.ids().is_empty(), "reject before starting either participant");
        assert_eq!(m.wb.cursor(), ROOT);
    }
}

#[test]
fn spec_01_unanswered_setup_has_a_bounded_retirement_even_after_stop() {
    // EngineManager.java:77–78,3385–3436: ordinary startup budget is 180 seconds.
    // Rules restoration has a 30-second bound (3233–3255), not an indefinite setup wait.
    for stop_before_expiry in [false, true] {
        let mut m = Match::new();
        let accepted = Instant::now();
        m.start(batch());
        m.serve(&|_, command| (command == "time_settings 0 2 1").then_some("HOLD".into()));
        assert_eq!(m.phase(), Some(MatchPhase::Starting));
        let document = m.wb.document().id;
        if stop_before_expiry {
            m.command(MatchCommand::Stop);
        }
        m.wb.engine_tick(accepted + Duration::from_secs(181));
        m.serve(&|_, _| None);
        m.wb.engine_tick(accepted + Duration::from_secs(212));
        m.serve(&|_, _| None);
        let view = m.wb.view(false);
        assert!(
            view.engine_match.is_none(),
            "unanswered setup cannot hold retirement forever"
        );
        assert_ne!(view.engine.lease, LeaseAvailabilityView::PlayMode);
        let outcome = view.match_outcome.expect("bounded startup/cancellation outcome");
        if !stop_before_expiry {
            assert_eq!(outcome.end, MatchEnd::Retired);
        }
        assert!(outcome.batch_finished);
        assert_eq!(
            m.wb.document().id,
            document,
            "a failed startup never starts a successor"
        );
    }
}

#[test]
fn spec_02_rules_restore_requires_matching_semantic_readback() {
    // EngineManager.java:3233–3255: acknowledgement alone is not a confirmed restoration.
    const JAPANESE: &str = r#"{"ko":"SIMPLE","scoring":"TERRITORY","tax":"SEKI","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#;
    for mismatch in [true, false] {
        let mut m = Match::new();
        m.start(batch());
        m.serve(&|_, _| None);
        let first = m.mover();
        m.answer(first, "genmove", "resign");
        let restored = m.serve(&|instance, command| {
            (mismatch && instance == first && command == "kata-get-rules").then(|| JAPANESE.to_string())
        });
        let outcome = m.wb.view(false).match_outcome.expect("restoration outcome");
        if mismatch {
            assert!(
                !outcome.unrestored.is_empty(),
                "a lying restore acknowledgement is not successful handback"
            );
            assert!(outcome.batch_finished);
            assert_eq!(m.phase(), None);
        } else {
            assert!(outcome.unrestored.is_empty());
            assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
        }
        for participant in m.world.ids() {
            assert!(
                restored
                    .iter()
                    .any(|(instance, command)| *instance == participant && command == "kata-get-rules"),
                "each captured original is verified after restoring"
            );
        }
    }
}

#[test]
fn spec_04_previous_only_first_candidate_means_the_move_actually_played() {
    // BoardRenderer.java:1610–1623 keeps the previous candidate matching the actual move,
    // not candidate zero; a pass or a move absent from that analysis has no displayed candidate.
    for played in [Some(Point { x: 15, y: 3 }), None, Some(Point { x: 2, y: 16 })] {
        let mut m = Match::new();
        m.command(MatchCommand::Start {
            settings: MatchSettings {
                rules: "chinese".into(),
                show_previous_bestmoves: true,
                previous_bestmove_only: true,
                ..MatchSettings::default()
            },
            session: MatchSessionSettings {
                kind: GameKind::Analysis,
                time_enabled: false,
                visits_enabled: true,
                first_visits: 1000,
                second_visits: 1000,
                auto_save: false,
                ..MatchSessionSettings::default()
            },
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        });
        m.serve(&|_, _| None);
        let first = m.world.ids()[0];
        m.out(first, "info move D4 visits 10 winrate 0.5 prior 0.5 order 0 pv D4 info move Q16 visits 5 winrate 0.49 prior 0.2 order 1 pv Q16");
        m.command(MatchCommand::Intervention {
            op: MatchIntervention::ToggleManual,
        });
        let edit = played.map_or(EditCommand::Pass, |point| EditCommand::Play { point, color: None });
        assert_eq!(m.wb.edit(edit).unwrap(), CommandResult::Done);
        m.serve(&|_, _| None);
        let analysis = m.wb.inspect().analysis.expect("previous root analysis");
        assert_eq!(analysis.node, ROOT);
        let candidates = analysis
            .candidates
            .iter()
            .map(|candidate| candidate.point)
            .collect::<Vec<_>>();
        let expected = if played == Some(Point { x: 15, y: 3 }) {
            vec![played]
        } else {
            Vec::new()
        };
        assert_eq!(
            candidates, expected,
            "filter by the committed coordinate, including non-first candidates"
        );
    }
}

#[test]
fn spec_05_enabled_zero_max_moves_uses_board_area_times_two() {
    // EngineGamePlan.java:180–186: enabled AND positive selects the configured maximum.
    let mut m = Match::new();
    m.start(MatchSessionSettings {
        max_moves_enabled: true,
        max_moves: 0,
        ..genmove()
    });
    m.serve(&|_, _| None);
    m.finish_turn("D4");
    m.finish_turn("Q16");
    assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 2);
    assert_eq!(
        m.phase(),
        Some(MatchPhase::Playing),
        "zero resolves to 19*19*2, not a zero-move game"
    );
    assert!(m.wb.inspect().match_outcome.is_none());

    // The match summary reports the limit: board width × height × 2 on each board size, so
    // neither "no limit" nor a fixed 722 passes for it. Java's English label is " Max moves:".
    for (size, expected) in [(9u8, 162u32), (19, 722)] {
        let profile = |id: &str, name: &str| EngineProfile {
            id: id.into(),
            name: name.into(),
            command: "/opt/katago gtp".into(),
            width: size.into(),
            height: size.into(),
            ..EngineProfile::default()
        };
        let mut m = Match::with_profiles(Some("en-US"), vec![profile("one", "First"), profile("two", "Second")]);
        m.start(MatchSessionSettings { max_moves_enabled: true, max_moves: 0, ..batch() });
        m.serve(&|_, _| None);
        assert_eq!(m.wb.document().record.size.width, u16::from(size));
        m.finish_turn("resign");
        let jobs = m.wb.take_background_saves();
        let summary = &jobs.iter().find(|j| j.path.extension().is_some_and(|e| e == "txt")).unwrap().text;
        let shown: String = summary
            .split_once(" Max moves:")
            .unwrap_or_else(|| panic!("{summary}"))
            .1
            .chars()
            .take_while(char::is_ascii_digit)
            .collect();
        assert_eq!(shown, expected.to_string(), "{size}×{size}");
    }
}

#[test]
fn spec_06_filling_a_no_capture_board_ends_immediately_in_both_modes() {
    // EngineManager.java:10250–10292 scopes full-board detection to no-capture;
    // Leelaz.java:5944,8932 checks it immediately after committing the filling move.
    for kind in [GameKind::Genmove, GameKind::Analysis] {
        let mut m = Match::new();
        let mut edit = m.wb.config().edit.clone();
        edit.no_capture = true;
        m.wb.set_edit_settings(edit).unwrap();
        m.command(MatchCommand::ReplaceOpenings {
            files: vec![MatchOpeningFile {
                path: "almost-full.sgf".into(),
                bytes: b"(;SZ[2]AB[aa][ba][ab]PL[B])".to_vec(),
            }],
        });
        m.command(MatchCommand::Start {
            settings: MatchSettings {
                rules: "chinese".into(),
                sgf_start: true,
                sgf_random: false,
                ..MatchSettings::default()
            },
            session: MatchSessionSettings {
                kind,
                time_enabled: false,
                visits_enabled: true,
                first_visits: 10,
                second_visits: 10,
                auto_save: false,
                ..MatchSessionSettings::default()
            },
            first_engine: 0,
            second_engine: 1,
            disable_wrn: true,
        });
        m.serve(&|_, _| None);
        let first = m.world.ids()[0];
        match kind {
            GameKind::Genmove => m.answer(first, "genmove", "B1"),
            GameKind::Analysis => m.out(first, "info move B1 visits 10 winrate 0.5 prior 0.5 order 0 pv B1"),
        }
        let sent = m.serve(&|_, _| None);
        let outcome =
            m.wb.view(false)
                .match_outcome
                .expect("filling move is terminal immediately");
        assert_eq!(outcome.end, MatchEnd::MaxMoves);
        assert_eq!(m.wb.document().record.node(m.wb.cursor()).move_number, 1);
        assert_eq!(m.phase(), None);
        assert!(!m.held.iter().any(|(_, _, command)| command.starts_with("genmove")));
        assert!(
            !sent.iter().any(|(_, command)| command.starts_with("kata-analyze")),
            "no next turn after filling the board"
        );
    }
    let mut m = Match::new();
    m.command(MatchCommand::ReplaceOpenings {
        files: vec![MatchOpeningFile {
            path: "full-setup.sgf".into(),
            bytes: b"(;SZ[2]AB[aa][ba][ab][bb]PL[W])".to_vec(),
        }],
    });
    m.command(MatchCommand::Start {
        settings: MatchSettings {
            rules: "chinese".into(),
            sgf_start: true,
            ..MatchSettings::default()
        },
        session: genmove(),
        first_engine: 0,
        second_engine: 1,
        disable_wrn: true,
    });
    m.serve(&|_, _| None);
    m.finish_turn("pass");
    assert_eq!(
        m.phase(),
        Some(MatchPhase::Playing),
        "normal-capture games do not use the no-capture full-board rule"
    );
}

#[test]
fn spec_07_saved_match_records_include_java_match_rules_sections() {
    // EngineManager.java:3274–3287, SGFParser.java:1213–1220 and
    // MatchRulesTexts.java:65–119,145–175; literals are from the English Java bundle.
    let mut m = Match::with_locale(Some("en-US"));
    m.start(batch());
    m.serve(&|_, _| None);
    m.finish_turn("resign");
    let jobs = m.wb.take_background_saves();
    let summary = jobs
        .iter()
        .find(|job| job.path.extension().is_some_and(|extension| extension == "txt"))
        .unwrap();
    assert_eq!(
        summary.text.matches("\r\nRules: Chinese").count(),
        2,
        "both participant settings include the target rules"
    );
    let sgf = jobs
        .iter()
        .find(|job| job.path.extension().is_some_and(|extension| extension == "sgf"))
        .unwrap();
    let record = lizzie_core::formats::parse(lizzie_core::record::RecordFormat::Sgf, &sgf.text, false).unwrap();
    let expected = concat!(
        "Match rules\nTarget: Chinese\nconfirmed\n",
        "Black: First (/opt/katago gtp)\nStatus: confirmed\n",
        "Effective rules: Chinese (ko=SIMPLE, scoring=AREA, tax=NONE, suicide=false, hasButton=false, whiteHandicapBonus=N, friendlyPassOk=true)\n",
        "White: Second (/opt/katago gtp)\nStatus: confirmed\n",
        "Effective rules: Chinese (ko=SIMPLE, scoring=AREA, tax=NONE, suicide=false, hasButton=false, whiteHandicapBonus=N, friendlyPassOk=true)\n",
        "End of match rules"
    );
    assert!(
        record.node(ROOT).comment.contains(expected),
        "the saved root preserves Java's complete confirmed-rules block"
    );
    assert_eq!(record.node(ROOT).comment.matches("End of match rules").count(), 1);
}

/// The saved record and the batch summary name each side's engine command (Java
/// MatchRulesTexts.identityLabel and EngineManager.fillEngineGameSettingStrings), but a command
/// carrying credentials is written with their values masked, as every persisted text is.
#[test]
fn saved_match_records_and_summaries_mask_credentials_in_engine_commands() {
    const MARK: &str = "R24MARK";
    let secret = format!(r#"/opt/katago gtp --token {MARK}tok -override-config "password={MARK}pw""#);
    let masked = r#"/opt/katago gtp --token <redacted> -override-config "password=<redacted>""#;
    let plain = "/opt/katago gtp -model plain.bin.gz";
    let mut m = Match::with_profiles(
        Some("en-US"),
        vec![
            EngineProfile { id: "one".into(), name: "First".into(), command: secret, ..EngineProfile::default() },
            EngineProfile { id: "two".into(), name: "Second".into(), command: plain.into(), ..EngineProfile::default() },
        ],
    );
    m.start(batch());
    m.serve(&|_, _| None);
    m.finish_turn("resign");
    let jobs = m.wb.take_background_saves();
    let sgf = jobs.iter().find(|j| j.path.extension().is_some_and(|e| e == "sgf")).unwrap();
    let summary = jobs.iter().find(|j| j.path.extension().is_some_and(|e| e == "txt")).unwrap();
    for (what, text) in [("sgf", &sgf.text), ("summary", &summary.text)] {
        assert!(!text.contains(MARK), "{what} holds a credential: {text}");
    }
    let record = lizzie_core::formats::parse(lizzie_core::record::RecordFormat::Sgf, &sgf.text, false).unwrap();
    let root = &record.node(ROOT).comment;
    assert!(root.contains(&format!("Black: First ({masked})\nStatus: confirmed\n")), "{root}");
    assert!(root.contains(&format!("White: Second ({plain})\nStatus: confirmed\n")), "{root}");
    assert!(sgf.text.contains("PB[First]") && sgf.text.contains("PW[Second]") && sgf.text.contains("RE[W+R]"));
    assert!(summary.text.contains(&format!("\r\nCommand:{masked}")), "{}", summary.text);
    assert!(summary.text.contains(&format!("\r\nCommand:{plain}")), "{}", summary.text);
}

/// Windows and UNC paths in engine commands are shown and saved as typed (`\\`, `\U`, `\u0041`
/// included); only the credential values beside them are masked.
#[test]
fn match_rules_keep_windows_and_unc_paths_in_engine_commands_as_typed() {
    const MARK: &str = "UNCMARK";
    let first = r"C:\Users\a\katago.exe gtp -override-config logDir=\\wsl.localhost\Ubuntu\logs";
    let second = format!(r"\\server\share\u0041\katago.exe gtp --token {MARK}tok");
    let masked = r"\\server\share\u0041\katago.exe gtp --token <redacted>";
    let mut m = Match::with_profiles(
        Some("en-US"),
        vec![
            EngineProfile { id: "one".into(), name: "First".into(), command: first.into(), ..EngineProfile::default() },
            EngineProfile { id: "two".into(), name: "Second".into(), command: second, ..EngineProfile::default() },
        ],
    );
    m.start(batch());
    m.serve(&|_, _| None);
    let shown = details(&m).unwrap();
    assert!(shown.contains(&format!("Black: First ({first})\nStatus: confirmed\n")), "{shown}");
    assert!(shown.contains(&format!("White: Second ({masked})\nStatus: confirmed\n")), "{shown}");
    assert!(!shown.contains(MARK), "{shown}");
    m.finish_turn("resign");
    let jobs = m.wb.take_background_saves();
    let sgf = jobs.iter().find(|j| j.path.extension().is_some_and(|e| e == "sgf")).unwrap();
    let summary = jobs.iter().find(|j| j.path.extension().is_some_and(|e| e == "txt")).unwrap();
    let record = lizzie_core::formats::parse(lizzie_core::record::RecordFormat::Sgf, &sgf.text, false).unwrap();
    let root = &record.node(ROOT).comment;
    assert!(root.contains(&shown), "the saved block is the details text: {root}");
    assert!(!sgf.text.contains(MARK) && !summary.text.contains(MARK));
    assert!(summary.text.contains(&format!("\r\nCommand:{first}")), "{}", summary.text);
    assert!(summary.text.contains(&format!("\r\nCommand:{masked}")), "{}", summary.text);
}

#[test]
fn spec_08_query_only_rules_are_checked_instead_of_offered_as_unverified() {
    // MatchRulesAdmission.java:36–40,167–180,202–210: a queryable side must return
    // complete matching rules, even without a setter; only non-queryable sides need consent.
    const JAPANESE: &str = r#"{"ko":"SIMPLE","scoring":"TERRITORY","tax":"SEKI","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#;
    for response in [JAPANESE, "{}", "?query failed", RULES] {
        let mut m = Match::new();
        m.start(genmove());
        let first = m.world.ids()[0];
        let commands = COMMANDS
            .lines()
            .filter(|command| *command != "kata-set-rules")
            .collect::<Vec<_>>()
            .join("\n");
        let sent = m.serve(&|instance, command| {
            if instance != first {
                return None;
            }
            match command {
                "list_commands" => Some(commands.clone()),
                "kata-get-rules" => Some(response.to_string()),
                _ => None,
            }
        });
        assert!(
            sent.iter()
                .any(|(instance, command)| *instance == first && command == "kata-get-rules"),
            "query-only participant is still queried"
        );
        assert!(!sent
            .iter()
            .any(|(instance, command)| *instance == first && command.starts_with("kata-set-rules")));
        if response == RULES {
            let view =
                m.wb.inspect()
                    .engine_match
                    .expect("matching query-only side is confirmed");
            assert_eq!(view.phase, MatchPhase::Playing);
            assert!(!view.rules_consent_required);
        } else {
            assert_eq!(
                m.phase(),
                None,
                "query mismatch/incomplete/failure cannot enter a consent path"
            );
            assert_eq!(m.wb.view(false).match_outcome.unwrap().end, MatchEnd::Retired);
        }
    }
}

fn participant(id: &str, name: &str) -> EngineProfile {
    EngineProfile { id: id.into(), name: name.into(), command: format!("/opt/{id} gtp"), ..EngineProfile::default() }
}

#[test]
fn a_custom_websocket_participant_takes_its_fixed_time_as_the_katago_move_limit() {
    let custom = EngineProfile {
        name: "Custom Compute".into(),
        command: "remote-compute://custom-websocket".into(),
        cloud: Some(lizzie_core::engine::cloud::CloudLaunch {
            provider: lizzie_core::engine::cloud::CloudProvider::Custom,
            identifier: String::new(),
            args: String::new(),
            url: "ws://127.0.0.1:1/analysis".into(),
        }),
        ..EngineProfile::default()
    };
    let mut m = Match::with_profiles(None, vec![custom, participant("two", "Second")]);
    m.start(genmove());
    let sent = m.serve(&|_, _| None);
    // Each side by its launch input: the 自建算力 process was launched for the cloud profile.
    let cloud: Vec<InstanceId> = m.world.0.lock().unwrap().iter().filter(|p| p.cloud).map(|p| p.id).collect();
    assert_eq!(cloud.len(), 1, "one 自建算力 participant");
    let setup = |custom: bool| -> Vec<&String> {
        sent.iter().filter(|(i, _)| cloud.contains(i) == custom).map(|(_, c)| c).collect()
    };
    let (remote, local) = (setup(true), setup(false));
    assert!(!local.is_empty(), "the local participant was set up");
    assert!(remote.iter().any(|c| *c == "kata-time_settings none"), "{remote:?}");
    assert!(remote.iter().any(|c| *c == "kata-set-param maxTime 2"), "{remote:?}");
    assert!(!remote.iter().any(|c| c.starts_with("time_settings")), "the 自建算力 side refuses time_settings: {remote:?}");
    assert!(local.iter().any(|c| *c == "time_settings 0 2 1"), "a local participant keeps its time settings: {local:?}");
    assert!(!local.iter().any(|c| c.starts_with("kata-set-param maxTime") || c.starts_with("kata-time_settings")), "{local:?}");
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    m.finish_turn("D4");
    m.finish_turn("Q16");
    let rec = &m.wb.document().record;
    let mut moves = 0;
    let mut node = ROOT;
    while let Some(next) = rec.main_child(node) {
        moves += matches!(rec.node(next).kind, NodeKind::Move { .. }) as usize;
        node = next;
    }
    assert_eq!(moves, 2, "both participants moved");
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
}

#[test]
fn a_fixed_handicap_is_set_only_on_a_nineteen_line_match_board() {
    for (size, handicap, stones, to_play) in [(9, 0, 0, Stone::Black), (19, 2, 2, Stone::White)] {
        let black = EngineProfile { width: size, height: size, ..participant("one", "First") };
        let mut m = Match::with_profiles(None, vec![black, participant("two", "Second")]);
        assert_eq!(m.wb.document().record.size.width, 19, "the open record is 19x19");
        let settings = MatchSettings { rules: "chinese".into(), handicap: 2, ..MatchSettings::default() };
        let start = MatchCommand::Start { settings, session: genmove(), first_engine: 0, second_engine: 1, disable_wrn: true };
        assert_eq!(m.command(start), CommandResult::Done);
        m.serve(&|_, _| None);
        let rec = &m.wb.document().record;
        assert_eq!((rec.size.width, rec.size.height), (size, size), "the black participant's board");
        assert_eq!(rec.info.handicap, handicap, "{size}x{size}: HA");
        assert_eq!(rec.position(ROOT).stones.iter().flatten().count(), stones, "{size}x{size}: handicap stones");
        assert_eq!(rec.node(ROOT).to_play, to_play, "{size}x{size}: side to play");
    }
}

#[test]
fn the_pda_dialog_reaches_the_kata_go_pda_participant_whichever_engine_is_in_front() {
    let fixed = |value: f64| CustomPdaSettings {
        mode: CustomPdaMode::Fixed,
        dynamic_cap: None,
        fixed_value: Some(value),
        auto_apply: false,
    };
    let mut m = Match::new();
    m.start(genmove());
    let world = m.world.clone();
    m.serve(&move |i, c| (c == "name" && i == world.ids()[0]).then(|| "KataGoPda 1.16.4".to_string()));
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    assert!(m.wb.inspect().engine_match.unwrap().kata_go_pda_participant);
    let (pda, standard) = (m.world.ids()[0], m.world.ids()[1]);
    // The standard KataGo is the one in front (the side to move).
    if m.mover() == pda {
        m.finish_turn("D4");
    }
    assert_eq!(m.mover(), standard);
    assert_eq!(m.wb.save_custom_pda(fixed(2.0)).unwrap(), None);
    let sent = |m: &Match, id| m.world.take(id).into_iter().map(|l| l.split_once(' ').unwrap().1.to_string()).collect::<Vec<_>>();
    assert_eq!(sent(&m, pda), ["pda 2.0"]);
    assert!(sent(&m, standard).is_empty(), "the standard participant gets no pda");

    // Paused, the same participant takes it.
    m.command(MatchCommand::Pause);
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    sent(&m, pda);
    sent(&m, standard);
    assert_eq!(m.wb.save_custom_pda(fixed(1.5)).unwrap(), None);
    assert_eq!(sent(&m, pda), ["pda 1.5"]);
    assert!(sent(&m, standard).is_empty());

    // Two standard KataGo participants: no KataGoPda flag, and no engine gets pda.
    let mut plain = Match::new();
    plain.start(genmove());
    plain.serve(&|_, _| None);
    assert!(!plain.wb.inspect().engine_match.unwrap().kata_go_pda_participant);
    assert_eq!(plain.wb.save_custom_pda(fixed(2.0)).unwrap(), None);
    for id in plain.world.ids() {
        assert!(sent(&plain, id).is_empty());
    }
}

// 本局规则 summary and details. Expected texts are Java's (`MatchRulesSnapshot.mainSummary`,
// `MatchRulesTexts.details`, `EngineGamePresentation.inspectableMatchRules`) in the English bundle.
const CHINESE_FIELDS: &str =
    "Chinese (ko=SIMPLE, scoring=AREA, tax=NONE, suicide=false, hasButton=false, whiteHandicapBonus=N, friendlyPassOk=true)";
const JAPANESE_RULES: &str = r#"{"ko":"SIMPLE","scoring":"TERRITORY","tax":"SEKI","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#;

fn caption(m: &Match) -> Option<String> {
    m.wb.inspect().match_rules_caption
}

fn details(m: &Match) -> Option<String> {
    match m.wb.inspect_match_rules() {
        CommandResult::MatchRulesDetails { text } => Some(text),
        CommandResult::Unchanged => None,
        other => panic!("{other:?}"),
    }
}

#[test]
fn without_a_game_the_rules_caption_stays_the_engines_and_nothing_is_inspected() {
    let mut m = Match::with_locale(Some("en-US"));
    assert_eq!(caption(&m), None);
    assert_eq!(details(&m), None);
    // A saved engine game reopened is a new record without the game's 本局规则 (its GameInfo has
    // no engine-game record); the block in its comment is only text.
    open_sgf(
        &mut m,
        "(;GM[1]SZ[19]DZ[KW]C[Match rules\nTarget: Chinese\nconfirmed\nEnd of match rules];B[pd])",
    );
    assert_eq!(caption(&m), None);
    assert_eq!(details(&m), None);
}

#[test]
fn confirmed_match_rules_name_the_target_and_list_each_engines_fields() {
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    // Preparing: both sides idle.
    assert_eq!(caption(&m).as_deref(), Some("Checking match rules"));
    assert_eq!(
        details(&m).as_deref(),
        Some(concat!(
            "Target: Chinese\nChecking match rules\n",
            "Black: First (/opt/katago gtp)\nStatus: pending\nEffective rules unavailable: pending\n",
            "White: Second (/opt/katago gtp)\nStatus: pending\nEffective rules unavailable: pending",
        ))
    );
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    assert_eq!(caption(&m).as_deref(), Some("Chinese"));
    let expected = format!(
        "Target: Chinese\nconfirmed\nBlack: First (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}\nWhite: Second (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}"
    );
    assert_eq!(details(&m), Some(expected.clone()));
    // Inspecting asks no engine anything.
    for id in m.world.ids() {
        m.world.take(id);
    }
    details(&m);
    for id in m.world.ids() {
        assert!(m.world.take(id).is_empty());
    }

    // The game ends and the board is reviewed: the record the game left on it is shown.
    m.finish_turn("resign");
    assert_eq!(m.phase(), None);
    assert_eq!(caption(&m).as_deref(), Some("Chinese"));
    assert_eq!(details(&m), Some(expected));
    // Another record has none.
    let new_board = |confirmed| EditCommand::NewBoard { confirmed };
    if let CommandResult::NeedsEditConfirmation { target } = m.wb.edit(new_board(None)).unwrap() {
        assert_eq!(m.wb.edit(new_board(Some(target))).unwrap(), CommandResult::Done);
    }
    assert_eq!(caption(&m), None);
    assert_eq!(details(&m), None);
}

#[test]
fn an_engine_that_cannot_query_rules_is_named_unconfirmed_with_the_reason() {
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    let first = m.world.ids()[0];
    let plain = "name\nversion\nlist_commands\nboardsize\nclear_board\nkomi\nplay\nundo\ngenmove";
    m.serve(&|instance, command| (instance == first && command == "list_commands").then(|| plain.to_string()));
    // Waiting for consent is still checking (Java publishes the game's rules after consent).
    let view = m.wb.inspect().engine_match.unwrap();
    assert!(view.rules_consent_required);
    assert_eq!(caption(&m).as_deref(), Some("Checking match rules"));
    m.command(MatchCommand::ConfirmRules {
        game: view.game,
        allow: true,
    });
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    assert_eq!(caption(&m).as_deref(), Some("Black: Unconfirmed / White: Chinese"));
    assert_eq!(
        details(&m),
        Some(format!(
            "Target: Chinese\nUnconfirmed\nBlack: First (/opt/katago gtp)\nStatus: Unconfirmed\nEffective rules unavailable: cannot query rules\nWhite: Second (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}"
        ))
    );
}

#[test]
fn a_failed_preparation_stays_inspectable_with_the_last_observation() {
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    let first = m.world.ids()[0];
    m.serve(&|instance, command| {
        (instance == first).then_some(())?;
        if command == "kata-get-rules" {
            Some(JAPANESE_RULES.to_string())
        } else if command == "kata-set-rules chinese" {
            // The overlay is refused; giving the engine its own rules back is not.
            Some("?cannot set rules".to_string())
        } else {
            None
        }
    });
    assert_eq!(m.phase(), None, "the start failed");
    let view = m.wb.view(false);
    assert_eq!(view.match_outcome.unwrap().end, MatchEnd::Retired);
    assert_eq!(
        view.match_notices
            .iter()
            .map(|n| n.message_key.as_str())
            .collect::<Vec<_>>(),
        ["MatchRules.failed.setFailed"]
    );
    assert_eq!(
        caption(&m).as_deref(),
        Some("Black: Japanese / Korean / White: Chinese")
    );
    assert_eq!(
        details(&m),
        Some(format!(
            "Target: Chinese\nMatch rules could not be prepared.\nBlack: First (/opt/katago gtp)\nStatus: setting rules rejected\nEffective rules unavailable: setting rules rejected\nLast observed (not current): Japanese / Korean (ko=SIMPLE, scoring=TERRITORY, tax=SEKI, suicide=false, hasButton=false, whiteHandicapBonus=0, friendlyPassOk=true)\nWhite: Second (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}"
        ))
    );
    // The failed start is shown until another game starts.
    m.start(genmove());
    assert_eq!(caption(&m).as_deref(), Some("Checking match rules"));
}

#[test]
fn a_query_that_fails_shows_the_engines_earlier_rules_as_not_current() {
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    let second = m.world.ids()[1];
    // The engine answered its start-up query, then refuses the match's.
    let asked = std::cell::Cell::new(0);
    m.serve(&|instance, command| {
        (instance == second && command == "kata-get-rules").then_some(())?;
        asked.set(asked.get() + 1);
        (asked.get() > 1).then(|| "?rules query refused".to_string())
    });
    assert_eq!(m.phase(), None);
    assert_eq!(
        m.wb.view(false)
            .match_notices
            .iter()
            .map(|n| n.message_key.as_str())
            .collect::<Vec<_>>(),
        ["MatchRules.failed.queryFailed"]
    );
    assert_eq!(caption(&m).as_deref(), Some("Black: Chinese / White: Chinese"));
    let text = details(&m).unwrap();
    assert!(
        text.ends_with(&format!(
            "White: Second (/opt/katago gtp)\nStatus: rules query rejected\nEffective rules unavailable: rules query rejected\nLast observed (not current): {CHINESE_FIELDS}"
        )),
        "{text}"
    );
}

#[test]
fn a_start_stopped_before_any_rules_answer_leaves_no_match_rules() {
    // Java kept 正在检查本局规则 after such a start; nothing is being checked any more.
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    assert_eq!(caption(&m).as_deref(), Some("Checking match rules"));
    m.command(MatchCommand::Stop);
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), None);
    assert_eq!(caption(&m), None);
    assert_eq!(details(&m), None);
}

// A start that ends while the rules are being prepared. The controlled KataGo keeps the rules it
// is given (Japanese at first), so the rules each engine holds after its handback can be read.
const JAPANESE_FIELDS: &str = "Japanese / Korean (ko=SIMPLE, scoring=TERRITORY, tax=SEKI, suicide=false, hasButton=false, whiteHandicapBonus=0, friendlyPassOk=true)";

#[derive(Default)]
struct KeptRules(std::cell::RefCell<std::collections::HashMap<InstanceId, String>>);
impl KeptRules {
    fn of(&self, instance: InstanceId) -> lizzie_core::engine::rules::KataRules {
        let rules = self.0.borrow().get(&instance).cloned();
        lizzie_core::engine::rules::KataRules::parse(rules.as_deref().unwrap_or(JAPANESE_RULES)).unwrap()
    }
    /// `kata-set-rules` changes the engine's rules; `kata-get-rules` reads them.
    fn run(&self, instance: InstanceId, command: &str) -> Option<String> {
        if let Some(rules) = command.strip_prefix("kata-set-rules ") {
            let rules = if rules == "chinese" { RULES } else { rules };
            self.0.borrow_mut().insert(instance, rules.into());
            return Some(String::new());
        }
        (command == "kata-get-rules").then(|| {
            let rules = self.0.borrow().get(&instance).cloned();
            rules.unwrap_or_else(|| JAPANESE_RULES.into())
        })
    }
}

fn japanese() -> lizzie_core::engine::rules::KataRules {
    lizzie_core::engine::rules::KataRules::parse(JAPANESE_RULES).unwrap()
}

/// Serves `kept` engines; `instance` answers in order, so from its match's own `kata-get-rules`
/// (its second, after the start-up one) on it keeps every command waiting. `refuse_query`
/// refuses that match query instead of answering it.
fn serve_holding_from_match_query(m: &mut Match, kept: &KeptRules, instance: InstanceId, refuse_query: bool) {
    let queries = std::cell::Cell::new(0);
    m.serve(&|i, command| {
        if i == instance && command == "kata-get-rules" {
            queries.set(queries.get() + 1);
        }
        if i == instance && queries.get() >= 2 {
            return Some("HOLD".into());
        }
        kept.run(i, command)
    });
    // The held match query is refused when it is answered.
    if refuse_query {
        let at = m.held.iter().position(|(i, _, c)| *i == instance && c == "kata-get-rules").unwrap();
        m.held[at].2 = "refuse".into();
    }
}

/// `instance` now answers what it kept waiting, in order; everything after is served at once.
fn answer_held(m: &mut Match, kept: &KeptRules, instance: InstanceId) -> Vec<(InstanceId, String)> {
    let held = std::mem::take(&mut m.held);
    let (mine, rest): (Vec<_>, Vec<_>) = held
        .into_iter()
        .partition(|(i, _, c)| *i == instance && !c.starts_with("genmove"));
    m.held = rest;
    for (_, id, command) in mine {
        let text = if command == "refuse" {
            "?rules query refused".to_string()
        } else {
            kept.run(instance, &command).unwrap_or_else(|| usual_answer(&command).into())
        };
        m.reply(instance, &id, &text);
    }
    m.serve(&|i, command| kept.run(i, command))
}

#[test]
fn a_start_stopped_after_its_rules_were_sent_gives_each_engine_its_own_rules_back() {
    let kept = KeptRules::default();
    let mut m = Match::with_locale(Some("en-US"));
    m.start(genmove());
    let [first, second] = [m.world.ids()[0], m.world.ids()[1]];
    // First is sent the match's query, the Chinese rules and their readback, and has answered
    // none of them when the start is stopped.
    serve_holding_from_match_query(&mut m, &kept, first, false);
    assert_eq!(m.phase(), Some(MatchPhase::Starting));
    m.command(MatchCommand::Stop);
    m.serve(&|i, command| kept.run(i, command));
    // It then answers them in order, taking the Chinese rules.
    let handback = answer_held(&mut m, &kept, first);
    assert_eq!(m.phase(), None);
    let view = m.wb.view(false);
    assert!(view.match_outcome.unwrap().unrestored.is_empty());
    assert_eq!(view.engine.unrestored, None);
    // The handback puts the rules the engine reported before back and reads them again.
    assert_eq!(kept.of(first), japanese(), "{handback:?}");
    let restore = handback
        .iter()
        .position(|(i, c)| *i == first && c.starts_with("kata-set-rules {"));
    assert!(
        restore.is_some_and(|at| handback[at..].iter().any(|(i, c)| *i == first && c == "kata-get-rules")),
        "{handback:?}"
    );
    // Second, which had answered everything, is given back the same way.
    assert_eq!(kept.of(second), japanese());
}

#[test]
fn an_engine_whose_rules_before_the_match_are_unknown_is_not_given_back_as_usable() {
    // The match's rules query is refused but the overlay is taken; with or without a stop, the
    // engine cannot be put back to its own rules.
    for stop in [false, true] {
        let kept = KeptRules::default();
        let mut m = Match::with_locale(Some("en-US"));
        m.start(genmove());
        let [first, second] = [m.world.ids()[0], m.world.ids()[1]];
        serve_holding_from_match_query(&mut m, &kept, first, true);
        if stop {
            m.command(MatchCommand::Stop);
            m.serve(&|i, command| kept.run(i, command));
        }
        answer_held(&mut m, &kept, first);
        assert_eq!(m.phase(), None, "stop {stop}");
        assert_ne!(kept.of(first), japanese(), "the overlay changed the engine (stop {stop})");
        let view = m.wb.view(false);
        assert_eq!(
            view.match_outcome.unwrap().unrestored,
            [UnrestoredReason::Restore],
            "stop {stop}"
        );
        assert_eq!(view.engine.unrestored, Some(UnrestoredReason::Restore), "stop {stop}");
        assert_eq!(kept.of(second), japanese(), "stop {stop}");
    }
}

#[test]
fn a_start_ending_after_a_rules_answer_keeps_it_as_the_last_observation() {
    // First answers the match's rules query and has the Chinese rules and their readback still
    // open; Second has its match query open. Then First's process ends, or the start is stopped.
    for disconnect in [true, false] {
        let kept = KeptRules::default();
        let mut m = Match::with_locale(Some("en-US"));
        m.start(genmove());
        let [first, second] = [m.world.ids()[0], m.world.ids()[1]];
        let queries = std::cell::RefCell::new(std::collections::HashMap::<InstanceId, usize>::new());
        m.serve(&|i, command| {
            let mut queries = queries.borrow_mut();
            let asked = queries.entry(i).or_default();
            *asked += usize::from(command == "kata-get-rules");
            let first_match_query = i == first && *asked == 2 && command == "kata-get-rules";
            (*asked >= 2 && !first_match_query)
                .then(|| "HOLD".to_string())
                .or_else(|| kept.run(i, command))
        });
        assert_eq!(m.phase(), Some(MatchPhase::Starting));
        assert_eq!(caption(&m).as_deref(), Some("Checking match rules"));
        if disconnect {
            m.wb.engine_input(first, EngineInput::Exited(Some(1)));
        } else {
            m.command(MatchCommand::Stop);
        }
        m.serve(&|i, command| kept.run(i, command));
        let expected_caption = "Black: Japanese / Korean / White: Japanese / Korean";
        let expected = format!(
            "Target: Chinese\nMatch rules could not be prepared.\nBlack: First (/opt/katago gtp)\nStatus: pending\nEffective rules unavailable: pending\nLast observed (not current): {JAPANESE_FIELDS}\nWhite: Second (/opt/katago gtp)\nStatus: pending\nEffective rules unavailable: pending\nLast observed (not current): {JAPANESE_FIELDS}"
        );
        assert_eq!(caption(&m).as_deref(), Some(expected_caption), "disconnect {disconnect}");
        assert_eq!(details(&m).as_deref(), Some(expected.as_str()), "disconnect {disconnect}");
        // Retired: the snapshot stays, and inspecting it asks no engine anything.
        if !disconnect {
            answer_held(&mut m, &kept, first);
        }
        answer_held(&mut m, &kept, second);
        m.wb.engine_tick(Instant::now() + Duration::from_secs(120));
        m.serve(&|i, command| kept.run(i, command));
        assert_eq!(m.phase(), None);
        for id in m.world.ids() {
            m.world.take(id);
        }
        assert_eq!(caption(&m).as_deref(), Some(expected_caption), "disconnect {disconnect}");
        assert_eq!(details(&m).as_deref(), Some(expected.as_str()), "disconnect {disconnect}");
        for id in m.world.ids() {
            assert!(m.world.take(id).is_empty());
        }
    }
}
