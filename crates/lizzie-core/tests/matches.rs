//! Independent expectations: Java EngineGameBatchState, EngineGameModule and Leelaz at the
//! fixed migration baseline. Controlled processes exercise the same public Workbench interface.
use lizzie_core::api::*;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{CustomPdaMode, CustomPdaSettings, EngineProfile, EngineStartup};
use lizzie_core::engine::runtime::{EngineInput, EngineIo, InstanceId, Launcher};
use lizzie_core::record::{DocumentEdit, NodeKind, Point, Stone, ROOT};
use lizzie_core::settings::{MatchOpeningParameters, MatchSettings, Storage};
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
    /// `kata-set-param` values each controlled KataGo holds; an unset one reads 1.5.
    params: std::collections::HashMap<(InstanceId, String), String>,
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
            params: Default::default(),
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
                    let mut words = cmd.split(' ');
                    let text = match words.next().unwrap() {
                        "kata-set-param" => {
                            let name = words.next().unwrap().to_string();
                            self.params.insert((instance, name), words.next().unwrap().into());
                            String::new()
                        }
                        "kata-get-param" => {
                            let name = words.next().unwrap().to_string();
                            self.params
                                .get(&(instance, name))
                                .cloned()
                                .unwrap_or_else(|| "1.5".into())
                        }
                        _ => usual_answer(cmd).to_string(),
                    };
                    if cmd.starts_with("kata-analyze") {
                        self.out(instance, &format!("={id}"));
                        self.streams.insert(instance);
                    } else {
                        self.reply(instance, id, &text);
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

/// 规则面板 确定 while an engine game holds the engines is refused with its message (the gate every
/// engine-taking command passes); nothing is saved.
#[test]
fn the_rules_panel_is_refused_during_an_engine_game() {
    let mut m = Match::new();
    m.start(genmove());
    m.serve(&|_, _| None);
    let command = lizzie_core::api::Command::SetKataRules {
        rules: RULES.into(),
        komi: 7.0,
        rule_set: Some("CHINESE_2027".into()),
        auto_load: true,
    };
    assert_eq!(
        m.wb.game_gate(&command),
        Some(CommandResult::Refused { message_key: "LizzieFrame.engineGameStopFirstHint".into() })
    );
    assert!(!m.wb.config().engine.auto_load_kata_rules);
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
    for (rules, name) in [
        (japanese(true, "SEKI"), "Japanese / Korean"),
        (japanese(false, "SEKI"), "Japanese / Korean"),
        (japanese(true, "NONE"), "Custom"),
    ] {
        let mut m = Match::with_locale(Some("en-US"));
        m.command(MatchCommand::Start {
            settings: MatchSettings {
                rules,
                ..MatchSettings::default()
            },
            session: genmove(),
            first_engine: 0,
            second_engine: 1,
        });
        let view = m.wb.inspect().engine_match.unwrap();
        assert_eq!(view.rules_target_name, name);
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
        opening: MatchOpeningParameters {
            first_pda: 2.5,
            ..MatchOpeningParameters::default()
        },
        ..MatchSettings::default()
    };
    m.command(MatchCommand::Start {
        settings,
        session: values,
        first_engine: 0,
        second_engine: 1,
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
        m.wb.document().record.node(ROOT).comment.contains("2.5"),
        "the mover's confirmed nonzero opening PDA is in the PK comment, not its value before the batch"
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
        let black = EngineProfile {
            width: size,
            height: size,
            ..participant("one", "First")
        };
        let mut m = Match::with_profiles(None, vec![black, participant("two", "Second")]);
        assert_eq!(m.wb.document().record.size.width, 19, "the open record is 19x19");
        let settings = MatchSettings {
            rules: "chinese".into(),
            handicap: 2,
            ..MatchSettings::default()
        };
        let start = MatchCommand::Start {
            settings,
            session: genmove(),
            first_engine: 0,
            second_engine: 1,
        };
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

    // Two standard KataGo participants: no engine gets pda.
    let mut plain = Match::new();
    plain.start(genmove());
    plain.serve(&|_, _| None);
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
fn a_start_whose_opening_values_fail_keeps_the_name_of_its_confirmed_rules() {
    let mut m = Match::with_locale(Some("en-US"));
    start_analysis(&mut m, OPENING, analysis());
    let one = m.world.ids()[0];
    m.serve(&move |i, c| (i == one && c == get(WRN)).then(|| "0.25".into()));
    assert_eq!(m.phase(), None);
    assert_eq!(notices(&m, "product.matchOpeningFailed").len(), 1);
    // Both engines confirmed the rules: the start failed for its parameters, not its rules.
    assert_eq!(caption(&m).as_deref(), Some("Chinese"));
    assert_eq!(
        details(&m),
        Some(format!(
            "Target: Chinese\nMatch rules could not be prepared.\nBlack: First (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}\nWhite: Second (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {CHINESE_FIELDS}"
        ))
    );
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

// ---- 对局参数会话: Java EngineGameParameterSession / EngineGameParticipantCommands -----------

const WRN: &str = "analysisWideRootNoise";
const PDA: &str = "playoutDoublingAdvantage";
/// 引擎 1 breadth 0.5 and aggressiveness 1, 引擎 2 breadth 0 and aggressiveness -1.
const OPENING: MatchOpeningParameters = MatchOpeningParameters {
    first_wrn: 0.5,
    second_wrn: 0.0,
    first_pda: 1.0,
    second_pda: -1.0,
};

fn analysis() -> MatchSessionSettings {
    MatchSessionSettings {
        kind: GameKind::Analysis,
        auto_save: false,
        exchange_colors: false,
        ..MatchSessionSettings::default()
    }
}

fn start_analysis(m: &mut Match, opening: MatchOpeningParameters, values: MatchSessionSettings) -> CommandResult {
    m.command(MatchCommand::Start {
        settings: MatchSettings {
            rules: "chinese".into(),
            opening,
            ..MatchSettings::default()
        },
        session: values,
        first_engine: 0,
        second_engine: 1,
    })
}

/// The breadth / aggressiveness commands among `sent`, in order.
fn parameter_commands(sent: &[(InstanceId, String)]) -> Vec<(InstanceId, String)> {
    sent.iter()
        .filter(|(_, c)| c.contains(WRN) || c.contains(PDA))
        .cloned()
        .collect()
}

/// Those the match sends: after its setup begins, past each engine's own start-up read-back
/// and the foreground analysis before the match took the engines.
fn match_commands(sent: &[(InstanceId, String)]) -> &[(InstanceId, String)] {
    let setup = sent
        .iter()
        .position(|(_, c)| c.starts_with("kata-set-rules"))
        .expect("the match sets its rules");
    &sent[setup..]
}

fn get(name: &str) -> String {
    format!("kata-get-param {name}")
}

fn set(name: &str, value: &str) -> String {
    format!("kata-set-param {name} {value}")
}

fn notices(m: &Match, key: &str) -> Vec<Vec<MatchParameterFailure>> {
    m.wb.inspect()
        .match_notices
        .into_iter()
        .filter(|n| n.message_key == key)
        .map(|n| n.parameter_failures)
        .collect()
}

fn failure(first: bool, reason: MatchParameterFailureReason) -> MatchParameterFailure {
    MatchParameterFailure { first: Some(first), reason }
}

#[test]
fn the_opening_sets_each_participant_breadth_then_aggressiveness_and_reads_them_back() {
    let mut m = Match::new();
    assert_eq!(start_analysis(&mut m, OPENING, analysis()), CommandResult::Done);
    let world = m.world.clone();
    // 引擎 2 already holds breadth 0: the 0 is still written and read back (Java applyOpening).
    let sent = m.serve(&move |i, c| (i == world.ids()[1] && c == get(WRN)).then(|| "0".into()));
    let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
    // Both processes are captured before the first write; breadth goes to both and is read back
    // before aggressiveness is written to either.
    let expected: Vec<(InstanceId, String)> = vec![
        (one, get(WRN)),
        (two, get(WRN)),
        (one, get(PDA)),
        (two, get(PDA)),
        (one, set(WRN, "0.5")),
        (two, set(WRN, "0")),
        (one, get(WRN)),
        (two, get(WRN)),
        (one, set(PDA, "1")),
        (two, set(PDA, "-1")),
        (one, get(PDA)),
        (two, get(PDA)),
    ];
    let sent = match_commands(&sent);
    assert_eq!(parameter_commands(sent), expected);
    let last = sent.iter().rposition(|(_, c)| c.contains(PDA)).unwrap();
    let search = sent
        .iter()
        .position(|(_, c)| c.starts_with("kata-analyze"))
        .expect("the game searches");
    assert!(
        search > last,
        "the game searches only after both participants confirmed"
    );
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    let p = m.wb.inspect().engine_match.unwrap().parameters.unwrap();
    assert!(!p.pending && p.failures.is_empty());
    let confirmed = |v: MatchParameterValue| (v.supported, v.confirmed, v.target);
    assert_eq!(confirmed(p.first.wrn), (true, Some(0.5), None));
    assert_eq!(confirmed(p.first.pda), (true, Some(1.0), None));
    assert_eq!(confirmed(p.second.wrn), (true, Some(0.0), None));
    assert_eq!(confirmed(p.second.pda), (true, Some(-1.0), None));
}

#[test]
fn a_genmove_match_leaves_breadth_and_aggressiveness_alone() {
    let mut m = Match::new();
    let start = MatchCommand::Start {
        settings: MatchSettings {
            rules: "chinese".into(),
            opening: OPENING,
            ..MatchSettings::default()
        },
        session: genmove(),
        first_engine: 0,
        second_engine: 1,
    };
    assert_eq!(m.command(start), CommandResult::Done);
    let sent = m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    assert!(!sent.iter().any(|(_, c)| c.starts_with("kata-set-param")), "{sent:?}");
    assert!(m.wb.inspect().engine_match.unwrap().parameters.is_none());
}

#[test]
fn the_end_of_the_batch_gives_each_process_its_values_from_before() {
    for stop in [true, false] {
        let mut m = Match::new();
        start_analysis(&mut m, OPENING, analysis());
        let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
        // 引擎 2 held breadth 0.25 before the batch.
        m.params.insert((two, WRN.into()), "0.25".into());
        m.serve(&|_, _| None);
        assert_eq!(m.params[&(one, WRN.to_string())], "0.5");
        if stop {
            m.command(MatchCommand::Stop);
        } else {
            // The single game ends; so does its batch.
            m.command(MatchCommand::Intervention {
                op: MatchIntervention::Resign { side: Stone::Black },
            });
        }
        let sent = m.serve(&|_, _| None);
        // One command at a time: each process, breadth then aggressiveness, set then read back.
        let expected: Vec<(InstanceId, String)> = vec![
            (one, set(WRN, "1.5")),
            (one, get(WRN)),
            (one, set(PDA, "1.5")),
            (one, get(PDA)),
            (two, set(WRN, "0.25")),
            (two, get(WRN)),
            (two, set(PDA, "1.5")),
            (two, get(PDA)),
        ];
        assert_eq!(parameter_commands(&sent), expected, "stop: {stop}");
        assert_eq!(m.params[&(one, WRN.to_string())], "1.5");
        assert_eq!(m.params[&(one, PDA.to_string())], "1.5");
        assert_eq!(m.params[&(two, PDA.to_string())], "1.5");
        // The engines are given back after the restore.
        for id in [one, two] {
            let restore = sent.iter().rposition(|(i, c)| *i == id && c.contains(PDA)).unwrap();
            let name = sent.iter().rposition(|(i, c)| *i == id && c == "name").unwrap();
            assert!(name > restore, "stop: {stop}");
        }
        assert_eq!(m.phase(), None, "stop: {stop}");
        assert!(notices(&m, "EngineGameParameters.restoreFailed").is_empty());
    }
}

#[test]
fn the_games_of_a_batch_keep_the_values_and_capture_each_process_once() {
    for stop_between in [false, true] {
        let mut m = Match::new();
        let values = MatchSessionSettings {
            batch_enabled: true,
            batch_limit: 2,
            exchange_colors: true,
            ..analysis()
        };
        start_analysis(&mut m, OPENING, values);
        m.serve(&|_, _| None);
        let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
        m.command(MatchCommand::Intervention {
            op: MatchIntervention::Resign { side: Stone::Black },
        });
        let between = m.serve(&|_, _| None);
        assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
        assert!(
            parameter_commands(&between).is_empty(),
            "nothing is restored between games: {between:?}"
        );
        if stop_between {
            // A batch stopped between its games takes its engines back for the restore.
            m.command(MatchCommand::Stop);
            let sent = m.serve(&|_, _| None);
            assert_eq!(
                parameter_commands(&sent)
                    .iter()
                    .filter(|(_, c)| c.starts_with("kata-set-param"))
                    .count(),
                4
            );
            assert_eq!(m.params[&(one, WRN.to_string())], "1.5");
            assert_eq!(m.phase(), None);
            continue;
        }
        m.wb.engine_tick(Instant::now() + Duration::from_secs(1));
        let second = m.serve(&|_, _| None);
        assert_eq!(m.phase(), Some(MatchPhase::Playing));
        // Positions, not colours: 引擎 1 keeps its values after the colours are exchanged.
        let expected: Vec<(InstanceId, String)> = vec![
            (one, set(WRN, "0.5")),
            (two, set(WRN, "0")),
            (one, get(WRN)),
            (two, get(WRN)),
            (one, set(PDA, "1")),
            (two, set(PDA, "-1")),
            (one, get(PDA)),
            (two, get(PDA)),
        ];
        assert_eq!(
            parameter_commands(&second),
            expected,
            "the same processes are not captured again"
        );
        m.command(MatchCommand::Intervention {
            op: MatchIntervention::Resign { side: Stone::Black },
        });
        let end = m.serve(&|_, _| None);
        assert_eq!(
            parameter_commands(&end)
                .iter()
                .filter(|(_, c)| c.starts_with("kata-set-param"))
                .count(),
            4
        );
        assert_eq!(
            (
                m.params[&(one, WRN.to_string())].as_str(),
                m.params[&(two, PDA.to_string())].as_str()
            ),
            ("1.5", "1.5")
        );
        assert_eq!(m.phase(), None);
    }
}

#[test]
fn an_unconfirmed_opening_names_the_participant_and_reason_and_the_game_never_searches() {
    use std::cell::Cell;
    let timeout = Duration::from_secs(9);
    let cases: Vec<(
        &str,
        Box<dyn Fn(&World) -> Box<dyn Fn(InstanceId, &str) -> Option<String>>>,
        Vec<MatchParameterFailure>,
    )> = vec![
        (
            "read-back mismatch",
            Box::new(|w: &World| {
                let one = w.ids()[0];
                Box::new(move |i, c| (i == one && c == get(WRN)).then(|| "0.25".into()))
            }),
            vec![failure(
                true,
                MatchParameterFailureReason::Mismatch {
                    actual: "0.25".into(),
                    expected: "0.5".into(),
                },
            )],
        ),
        (
            "set refused",
            Box::new(|w: &World| {
                let one = w.ids()[0];
                Box::new(move |i, c| (i == one && c == set(WRN, "0.5")).then(|| "?unknown parameter".into()))
            }),
            vec![failure(
                true,
                MatchParameterFailureReason::Rejected {
                    command: set(WRN, "0.5"),
                    response: "unknown parameter".into(),
                },
            )],
        ),
        (
            "read-back timeout",
            Box::new(|w: &World| {
                let two = w.ids()[1];
                let written = Cell::new(false);
                let held = Cell::new(false);
                Box::new(move |i, c| {
                    if i == two && c.starts_with(&set(PDA, "")) {
                        written.set(true);
                    }
                    // The first read-back after the write does not answer.
                    let hold = i == two && c == get(PDA) && written.get() && !held.replace(true);
                    hold.then(|| "HOLD".into())
                })
            }),
            vec![failure(false, MatchParameterFailureReason::Timeout)],
        ),
    ];
    for (case, answer, expected) in cases {
        let mut m = Match::new();
        start_analysis(&mut m, OPENING, analysis());
        let answer = answer(&m.world);
        let mut sent = m.serve(&*answer);
        if case == "read-back timeout" {
            assert_eq!(m.phase(), Some(MatchPhase::Starting), "{case}: waiting for the answer");
            m.wb.engine_tick(Instant::now() + timeout);
            sent.extend(m.serve(&*answer));
        }
        assert_eq!(notices(&m, "product.matchOpeningFailed"), vec![expected], "{case}");
        let searches = |c: &str| c.starts_with("kata-analyze") || c.starts_with("genmove");
        assert!(
            !match_commands(&sent).iter().any(|(_, c)| searches(c)),
            "{case}: no search"
        );
        assert_eq!(m.phase(), None, "{case}: the batch ends");
        let outcome = m.wb.view(false).match_outcome.unwrap();
        assert_eq!(outcome.end, MatchEnd::Retired, "{case}");
        // The values written before the failure are given back.
        let one = m.world.ids()[0];
        let pda = m.params.get(&(one, PDA.to_string())).map(String::as_str);
        assert_eq!(
            pda,
            Some("1.5"),
            "{case}: {:?}",
            parameter_commands(match_commands(&sent))
        );
    }
}

#[test]
fn an_opening_cut_short_by_a_stop_or_an_ended_game_names_who_still_owed_an_answer() {
    use std::cell::Cell;
    for stop in [true, false] {
        let mut m = Match::new();
        start_analysis(&mut m, OPENING, analysis());
        let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
        // 引擎 2 does not answer its aggressiveness read-back.
        let written = Cell::new(false);
        let held = Cell::new(false);
        let answer = move |i: InstanceId, c: &str| {
            if i == two && c.starts_with(&set(PDA, "")) {
                written.set(true);
            }
            let hold = i == two && c == get(PDA) && written.get() && !held.replace(true);
            hold.then(|| "HOLD".into())
        };
        let sent = m.serve(&answer);
        assert_eq!(m.phase(), Some(MatchPhase::Starting), "waiting for 引擎 2");
        let searches = |c: &str| c.starts_with("kata-analyze") || c.starts_with("genmove");
        assert!(
            !match_commands(&sent).iter().any(|(_, c)| searches(c)),
            "no search: {sent:?}"
        );
        let expected = if stop {
            m.command(MatchCommand::Stop);
            MatchParameterFailureReason::Interrupted
        } else {
            m.wb.engine_input(one, EngineInput::Exited(Some(1)));
            MatchParameterFailureReason::GameEnded
        };
        // Afterwards the engines go back to the foreground, which may analyse again.
        m.serve(&answer);
        m.wb.engine_tick(Instant::now() + Duration::from_secs(30));
        m.serve(&answer);
        assert_eq!(
            notices(&m, "product.matchOpeningFailed"),
            vec![vec![failure(false, expected)]],
            "stop: {stop}"
        );
        assert_eq!(m.phase(), None, "stop: {stop}");
        if stop {
            assert_eq!(
                m.params[&(one, PDA.to_string())],
                "1.5",
                "引擎 1 is given its value back"
            );
        }
    }
}

/// Answers 引擎 2's `name` with `name`; a plain aggressiveness query made of it after its own
/// start-up read-back is never answered.
fn second_engine_named(world: World, name: &'static str) -> impl Fn(InstanceId, &str) -> Option<String> {
    let reads = std::cell::Cell::new(0);
    move |i, c| {
        if i != world.ids()[1] {
            return None;
        }
        if c == "name" {
            return Some(name.into());
        }
        if c == get(PDA) {
            reads.set(reads.get() + 1);
            return (reads.get() > 1).then(|| "HOLD".into());
        }
        None
    }
}

#[test]
fn a_participant_that_cannot_take_a_nonzero_value_is_refused_before_any_command() {
    let cases = [
        (
            "Leela Zero",
            MatchOpeningParameters {
                second_wrn: 0.5,
                ..MatchOpeningParameters::default()
            },
            MatchParameter::Wrn,
        ),
        (
            "KataGoPda 1.16.4",
            MatchOpeningParameters {
                second_pda: 1.0,
                ..MatchOpeningParameters::default()
            },
            MatchParameter::Pda,
        ),
    ];
    for (name, opening, parameter) in cases {
        let mut m = Match::new();
        start_analysis(&mut m, opening, analysis());
        let sent = m.serve(&second_engine_named(m.world.clone(), name));
        assert_eq!(
            notices(&m, "product.matchOpeningFailed"),
            vec![vec![failure(
                false,
                MatchParameterFailureReason::Unsupported { parameter }
            )]],
            "{name}"
        );
        assert!(
            parameter_commands(match_commands(&sent)).is_empty(),
            "{name}: nothing about the parameters goes to either engine: {sent:?}"
        );
        assert!(!sent.iter().any(|(_, c)| c.starts_with("kata-set-param")), "{name}");
        assert_eq!(m.phase(), None, "{name}");
    }
    // Zero is left alone: a KataGoPda participant takes breadth only, and its own aggressiveness
    // is neither read nor written, so a query it would never answer cannot hold the start.
    let mut m = Match::new();
    start_analysis(
        &mut m,
        MatchOpeningParameters {
            second_wrn: 0.5,
            ..MatchOpeningParameters::default()
        },
        analysis(),
    );
    let sent = m.serve(&second_engine_named(m.world.clone(), "KataGoPda 1.16.4"));
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    let two = m.world.ids()[1];
    let to_two: Vec<String> = parameter_commands(match_commands(&sent))
        .into_iter()
        .filter(|(i, _)| *i == two)
        .map(|(_, c)| c)
        .collect();
    assert_eq!(to_two, [get(WRN), set(WRN, "0.5"), get(WRN)]);
    assert!(
        match_commands(&sent).iter().any(|(_, c)| c.starts_with("kata-analyze")),
        "the game searches: {sent:?}"
    );
    let p = m.wb.inspect().engine_match.unwrap().parameters.unwrap();
    assert!(p.second.wrn.supported && !p.second.pda.supported && p.first.pda.supported);
}

#[test]
fn a_kata_go_pda_participant_comments_the_aggressiveness_it_reports_itself() {
    let mut m = Match::new();
    start_analysis(&mut m, MatchOpeningParameters::default(), analysis());
    let world = m.world.clone();
    let sent = m.serve(&move |i, c| match c {
        _ if i != world.ids()[0] => None,
        "name" => Some("KataGoPda 1.16.4".into()),
        "getpda" => Some("pda: 0.25".into()),
        _ => None,
    });
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    let first = m.world.ids()[0];
    assert_eq!(
        sent.iter().filter(|(i, c)| *i == first && *c == get(PDA)).count(),
        1,
        "only its own start-up read-back asks it for the plain value: {sent:?}"
    );
    m.out(
        first,
        "info move D4 visits 10 winrate 0.5 scoreLead 0.5 prior 0.5 order 0 pv D4",
    );
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::PlayNow,
    });
    m.serve(&|_, _| None);
    let comment = &m.wb.document().record.node(ROOT).comment;
    assert!(comment.contains("0.25"), "{comment}");
}

#[test]
fn a_process_that_ended_is_not_written_and_a_failed_restore_stops_its_process() {
    // The captured process ended: its replacement is never written; the other is restored.
    let mut m = Match::new();
    start_analysis(
        &mut m,
        OPENING,
        MatchSessionSettings {
            batch_enabled: true,
            batch_limit: 2,
            ..analysis()
        },
    );
    m.serve(&|_, _| None);
    let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
    m.wb.engine_input(two, EngineInput::Exited(Some(1)));
    let sent = m.serve(&|_, _| None);
    assert_eq!(m.phase(), None);
    assert_eq!(
        notices(&m, "EngineGameParameters.restoreFailed"),
        vec![vec![failure(false, MatchParameterFailureReason::ProcessChanged)]]
    );
    let replacement = *m.world.ids().last().unwrap();
    assert!(
        replacement != one && replacement != two,
        "引擎 2 starts again after the match"
    );
    assert!(
        !sent
            .iter()
            .any(|(i, c)| *i == replacement && c.starts_with("kata-set-param")),
        "{sent:?}"
    );
    assert_eq!(m.params[&(one, WRN.to_string())], "1.5", "引擎 1 is restored");

    // A refused restore is reported and that exact process is stopped instead of given back.
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let one = m.world.ids()[0];
    m.command(MatchCommand::Stop);
    let sent = m.serve(&move |i, c| (i == one && c == set(WRN, "1.5")).then(|| "?busy".into()));
    assert_eq!(
        notices(&m, "EngineGameParameters.restoreFailed"),
        vec![vec![failure(
            true,
            MatchParameterFailureReason::Rejected {
                command: set(WRN, "1.5"),
                response: "busy".into()
            }
        )]]
    );
    let after = sent
        .iter()
        .position(|(i, c)| *i == one && *c == set(WRN, "1.5"))
        .unwrap();
    assert!(
        !sent[after + 1..].iter().any(|(i, _)| *i == one),
        "nothing else goes to it: {sent:?}"
    );
    let engines = m.wb.inspect().engine.engines;
    assert!(engines[0].state.is_none(), "引擎 1 is stopped");
    assert!(engines[1].state.is_some(), "引擎 2 was restored and keeps running");
    assert!(
        m.wb.view(false).match_outcome.unwrap().unrestored.is_empty(),
        "reported once, by the restore"
    );
    assert_eq!(m.phase(), None);
}

#[test]
fn a_new_batch_starts_only_after_the_previous_one_gave_its_values_back() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let one = m.world.ids()[0];
    m.command(MatchCommand::Stop);
    m.serve(&move |i, c| (i == one && c == set(PDA, "1.5")).then(|| "HOLD".into()));
    assert_eq!(m.phase(), Some(MatchPhase::Ending), "the restore waits for 引擎 1");
    assert!(matches!(
        start_analysis(&mut m, OPENING, analysis()),
        CommandResult::Refused { .. }
    ));
    assert!(
        parameter_commands(&m.serve(&|_, _| None)).is_empty(),
        "no opening of the new batch yet"
    );
    m.params.insert((one, PDA.into()), "1.5".into());
    m.answer(one, &set(PDA, "1.5"), "");
    let restored = m.serve(&|_, _| None);
    assert_eq!(parameter_commands(&restored).last().unwrap().1, get(PDA));
    assert_eq!(m.phase(), None);
    assert!(notices(&m, "EngineGameParameters.restoreFailed").is_empty());
    assert_eq!(m.world.ids().len(), 2, "both processes stay up");
    assert_eq!(start_analysis(&mut m, OPENING, analysis()), CommandResult::Done);
    let sent = m.serve(&|_, _| None);
    let first = parameter_commands(match_commands(&sent))
        .first()
        .map(|(_, c)| c.clone());
    assert_eq!(first, Some(get(WRN)), "the new batch captures first: {sent:?}");
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
}

fn batch_of_two() -> MatchSessionSettings {
    MatchSessionSettings {
        batch_enabled: true,
        batch_limit: 2,
        ..analysis()
    }
}

#[test]
fn a_stop_while_a_game_gives_its_engines_back_restores_both_processes_afterwards() {
    // The game's rules handback: its own rules, their read-back, then `name`.
    fn give_back(m: &mut Match, instance: InstanceId, commands: &[&str]) {
        for &command in commands {
            let text = match command {
                "kata-get-rules" => RULES,
                "name" => "KataGo",
                _ => "",
            };
            m.answer(instance, command, text);
        }
    }
    const ALL: [&str; 3] = ["kata-set-rules {", "kata-get-rules", "name"];
    for other_seat_first in [false, true] {
        let mut m = Match::new();
        start_analysis(&mut m, OPENING, batch_of_two());
        let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
        m.params.insert((one, PDA.into()), "0.75".into());
        m.params.insert((two, WRN.into()), "0.25".into());
        m.serve(&|_, _| None);
        // The first game ends; its engines are being given back for the second one.
        m.command(MatchCommand::Intervention {
            op: MatchIntervention::Resign { side: Stone::Black },
        });
        m.serve(&|_, c| {
            (c.starts_with("kata-set-rules {") || c == "kata-get-rules" || c == "name").then(|| "HOLD".into())
        });
        assert_eq!(m.phase(), Some(MatchPhase::Ending));
        if other_seat_first {
            give_back(&mut m, two, &ALL[..2]);
        }
        assert_eq!(m.command(MatchCommand::Stop), CommandResult::Done);
        let mut sent = m.serve(&|_, _| None);
        if other_seat_first {
            give_back(&mut m, two, &ALL[2..]);
            give_back(&mut m, one, &ALL);
        } else {
            give_back(&mut m, one, &ALL);
            give_back(&mut m, two, &ALL);
        }
        sent.extend(m.serve(&|_, _| None));
        m.wb.engine_tick(Instant::now() + Duration::from_secs(1));
        sent.extend(m.serve(&|_, _| None));
        let case = format!("other seat first: {other_seat_first}");
        assert_eq!(m.phase(), None, "{case}");
        assert!(
            !sent.iter().any(|(_, c)| c == "kata-set-rules chinese"),
            "{case}: no second game: {sent:?}"
        );
        assert_eq!(
            parameter_commands(&sent),
            vec![
                (one, set(WRN, "1.5")),
                (one, get(WRN)),
                (one, set(PDA, "0.75")),
                (one, get(PDA)),
                (two, set(WRN, "0.25")),
                (two, get(WRN)),
                (two, set(PDA, "1.5")),
                (two, get(PDA)),
            ],
            "{case}: each process gets its values from before the batch back"
        );
        assert!(notices(&m, "EngineGameParameters.restoreFailed").is_empty(), "{case}");
        assert!(m.wb.view(false).match_outcome.unwrap().unrestored.is_empty(), "{case}");
        let engines = m.wb.inspect().engine.engines;
        assert!(
            engines.iter().all(|e| e.state.is_some()),
            "{case}: both processes keep running"
        );
        assert_eq!(m.world.ids(), [one, two], "{case}: no process was replaced");
    }
}

#[test]
fn the_start_up_limit_of_a_game_gives_both_processes_their_values_back_first() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, batch_of_two());
    let (one, two) = (m.world.ids()[0], m.world.ids()[1]);
    m.params.insert((two, WRN.into()), "0.25".into());
    m.serve(&|_, _| None);
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::Resign { side: Stone::Black },
    });
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
    // The second game: 引擎 1 does not answer its rules query before the start-up limit.
    let next = Instant::now() + Duration::from_secs(1);
    m.wb.engine_tick(next);
    let held = std::cell::Cell::new(false);
    m.serve(&|i, c| (i == one && c == "kata-get-rules" && !held.replace(true)).then(|| "HOLD".into()));
    assert_eq!(m.phase(), Some(MatchPhase::Starting));
    m.wb.engine_tick(next + Duration::from_secs(181));
    let mut sent = m.serve(&|_, _| None);
    m.answer(one, "kata-get-rules", RULES);
    sent.extend(m.serve(&|_, _| None));
    assert_eq!(
        parameter_commands(&sent),
        vec![
            (one, set(WRN, "1.5")),
            (one, get(WRN)),
            (one, set(PDA, "1.5")),
            (one, get(PDA)),
            (two, set(WRN, "0.25")),
            (two, get(WRN)),
            (two, set(PDA, "1.5")),
            (two, get(PDA)),
        ],
        "each process gets its values from before the batch back"
    );
    assert_eq!(m.params[&(two, WRN.to_string())], "0.25");
    assert!(notices(&m, "EngineGameParameters.restoreFailed").is_empty());
    assert_eq!(m.phase(), None);
    let engines = m.wb.inspect().engine.engines;
    assert!(engines.iter().all(|e| e.state.is_some()), "both processes keep running");
}

/// 对局参数 for the current game: [引擎 1 广度, 引擎 2 广度, 引擎 1 激进度, 引擎 2 激进度].
fn revise(m: &mut Match, values: [Option<f64>; 4]) -> CommandResult {
    let game = m.wb.inspect().engine_match.unwrap().game;
    let [first_wrn, second_wrn, first_pda, second_pda] = values;
    m.command(MatchCommand::ReviseParameters {
        game,
        first_wrn,
        second_wrn,
        first_pda,
        second_pda,
    })
}

fn parameters(m: &Match) -> MatchParametersView {
    m.wb.inspect().engine_match.unwrap().parameters.unwrap()
}

fn refusal(result: CommandResult) -> String {
    match result {
        CommandResult::Refused { message_key } => message_key,
        other => panic!("not refused: {other:?}"),
    }
}

#[test]
fn a_revision_writes_and_reads_back_only_the_changed_value_of_that_participant() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let two = m.world.ids()[1];
    // 引擎 2's breadth from 0 to 0.5.
    assert!(matches!(revise(&mut m, [None, Some(0.5), None, None]), CommandResult::ParametersRevised { .. }));
    let p = parameters(&m);
    assert!(p.pending);
    // The value being written is unknown until its read-back (Java `runUpdate` clears it).
    assert_eq!((p.second.wrn.confirmed, p.second.wrn.target), (None, Some(0.5)));
    let sent = m.serve(&|_, _| None);
    assert_eq!(
        parameter_commands(&sent),
        vec![(two, set(WRN, "0.5")), (two, get(WRN))]
    );
    let last = sent.iter().rposition(|(_, c)| c.contains(WRN)).unwrap();
    assert!(
        sent[last..].iter().any(|(_, c)| c.starts_with("kata-analyze")),
        "the game searches again once confirmed: {sent:?}"
    );
    let p = parameters(&m);
    assert!(!p.pending && p.failures.is_empty());
    assert_eq!((p.second.wrn.confirmed, p.second.wrn.target), (Some(0.5), None));
    assert_eq!((p.first.wrn.confirmed, p.first.pda.confirmed, p.second.pda.confirmed), (Some(0.5), Some(1.0), Some(-1.0)));
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
}

#[test]
fn a_revision_without_a_changed_value_sends_nothing() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    assert_eq!(revise(&mut m, [None; 4]), CommandResult::Unchanged);
    // The values the engines confirmed at the opening.
    assert_eq!(
        revise(&mut m, [Some(0.5), Some(0.0), Some(1.0), Some(-1.0)]),
        CommandResult::Unchanged
    );
    assert!(parameter_commands(&m.serve(&|_, _| None)).is_empty());
    assert!(!parameters(&m).pending);
    // Out of range, or no game of that number: refused before anything is recorded.
    assert_eq!(refusal(revise(&mut m, [None, Some(2.5), None, None])), "EngineGameParameters.invalidWrn");
    assert_eq!(refusal(revise(&mut m, [None, None, Some(-3.5), None])), "EngineGameParameters.invalidPda");
    let game = m.wb.inspect().engine_match.unwrap().game;
    let stale = MatchCommand::ReviseParameters {
        game: game + 1,
        first_wrn: Some(1.0),
        second_wrn: None,
        first_pda: None,
        second_pda: None,
    };
    assert_eq!(refusal(m.command(stale)), "EngineGameParameters.notPlaying");
    assert!(parameter_commands(&m.serve(&|_, _| None)).is_empty());
}

#[test]
fn a_read_back_mismatch_pauses_the_game_until_a_revision_is_confirmed() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let two = m.world.ids()[1];
    revise(&mut m, [None, Some(0.5), None, None]);
    m.serve(&move |i, c| (i == two && c == get(WRN)).then(|| "0.25".into()));
    let expected = vec![failure(
        false,
        MatchParameterFailureReason::Mismatch {
            actual: "0.25".into(),
            expected: "0.5".into(),
        },
    )];
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    let p = parameters(&m);
    assert!(!p.pending);
    assert_eq!(p.failures, expected);
    assert_eq!(p.second.wrn.confirmed, None, "the value is unknown");
    assert_eq!(notices(&m, "EngineGameParameters.failed"), vec![expected.clone()]);
    // 继续 is refused while the values are not confirmed.
    assert_eq!(refusal(m.command(MatchCommand::Pause)), "EngineGameParameters.resumeBlocked");
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    // Applying the same value again writes it again; once confirmed the game may continue.
    assert!(matches!(revise(&mut m, [None, Some(0.5), None, None]), CommandResult::ParametersRevised { .. }));
    let sent = m.serve(&|_, _| None);
    assert_eq!(
        parameter_commands(&sent),
        vec![(two, set(WRN, "0.5")), (two, get(WRN))]
    );
    let p = parameters(&m);
    assert!(!p.pending && p.failures.is_empty());
    assert_eq!(p.second.wrn.confirmed, Some(0.5));
    assert_eq!(m.phase(), Some(MatchPhase::Paused), "a revision does not resume the game");
    assert_eq!(m.command(MatchCommand::Pause), CommandResult::Done);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
}

#[test]
fn a_late_refusal_of_a_timed_out_revision_leaves_the_paused_game_as_it_was() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let two = m.world.ids()[1];
    let game = m.wb.inspect().engine_match.unwrap().game;
    revise(&mut m, [None, Some(0.5), None, None]);
    m.serve(&move |i, c| (i == two && c == set(WRN, "0.5")).then(|| "HOLD".into()));
    m.wb.engine_tick(Instant::now() + Duration::from_secs(9));
    m.serve(&|_, _| None);
    let timeout = vec![failure(false, MatchParameterFailureReason::Timeout)];
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    assert_eq!(parameters(&m).failures, timeout);
    // The engine refuses the timed-out write only now: that answer belongs to nobody.
    m.answer(two, "kata-set-param", "?too late");
    let after = m.serve(&|_, _| None);
    let view = m.wb.inspect().engine_match.unwrap();
    assert_eq!((view.game, view.phase), (game, MatchPhase::Paused));
    assert_eq!(parameters(&m).failures, timeout, "the reason stays the time-out");
    assert_eq!(m.wb.view(false).match_outcome, None, "the game did not end");
    assert!(parameter_commands(&after).is_empty(), "nothing is restored: {after:?}");
    // A new submission is confirmed and the game continues.
    assert!(matches!(revise(&mut m, [None, Some(0.5), None, None]), CommandResult::ParametersRevised { .. }));
    let sent = m.serve(&|_, _| None);
    assert_eq!(parameter_commands(&sent), vec![(two, set(WRN, "0.5")), (two, get(WRN))]);
    let p = parameters(&m);
    assert!(!p.pending && p.failures.is_empty());
    assert_eq!(m.command(MatchCommand::Pause), CommandResult::Done);
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
}

#[test]
fn a_revision_waiting_for_a_komi_change_too_long_fails_for_the_game() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    m.command(MatchCommand::Pause);
    // A komi change is waiting for the engines' answers.
    assert!(m.wb.set_komi(6.5).unwrap());
    m.serve(&|_, c| (c == "komi 6.5").then(|| "HOLD".into()));
    assert!(matches!(revise(&mut m, [None, Some(0.5), None, None]), CommandResult::ParametersRevised { .. }));
    assert!(parameter_commands(&m.serve(&|_, _| None)).is_empty(), "nothing is written meanwhile");
    assert!(parameters(&m).pending);
    m.wb.engine_tick(Instant::now() + Duration::from_secs(31));
    let expected = vec![MatchParameterFailure {
        first: None,
        reason: MatchParameterFailureReason::TurnTimeout,
    }];
    let p = parameters(&m);
    assert!(!p.pending);
    assert_eq!(p.failures, expected);
    assert_eq!(notices(&m, "EngineGameParameters.failed"), vec![expected]);
    assert_eq!(m.phase(), Some(MatchPhase::Paused));
    assert!(parameter_commands(&m.serve(&|_, _| None)).is_empty());
}

#[test]
fn a_game_ending_before_the_engines_answer_keeps_the_reason_with_its_parameters() {
    for stop in [false, true] {
        let mut m = Match::new();
        start_analysis(&mut m, OPENING, analysis());
        m.serve(&|_, _| None);
        let two = m.world.ids()[1];
        revise(&mut m, [None, Some(0.5), None, None]);
        // 引擎 2 does not answer the write.
        m.serve(&move |i, c| (i == two && c == set(WRN, "0.5")).then(|| "HOLD".into()));
        let expected = if stop {
            m.command(MatchCommand::Stop);
            MatchParameterFailureReason::Interrupted
        } else {
            m.command(MatchCommand::Intervention {
                op: MatchIntervention::Resign { side: Stone::Black },
            });
            MatchParameterFailureReason::GameEnded
        };
        // The batch holds the parameters until it gave them back; until then the game's
        // parameters say why the value was not confirmed.
        let view = m.wb.inspect().engine_match.expect("the batch is giving its values back");
        assert_eq!(view.phase, MatchPhase::Ending, "stop: {stop}");
        let p = view.parameters.unwrap();
        assert!(!p.pending, "stop: {stop}");
        assert_eq!(p.failures, vec![failure(false, expected)], "stop: {stop}");
        assert!(notices(&m, "EngineGameParameters.failed").is_empty(), "the game is over: no pause");
        m.answer(two, &set(WRN, "0.5"), "");
        m.serve(&|_, _| None);
        assert_eq!(m.phase(), None, "stop: {stop}");
        assert_eq!(m.params[&(two, WRN.to_string())], "1.5", "stop: {stop}: given back");
    }
}

#[test]
fn the_next_game_of_a_batch_starts_from_the_opening_values_and_refuses_the_last_games_revision() {
    let mut m = Match::new();
    let values = MatchSessionSettings {
        batch_enabled: true,
        batch_limit: 2,
        ..analysis()
    };
    start_analysis(&mut m, OPENING, values);
    m.serve(&|_, _| None);
    let previous = m.wb.inspect().engine_match.unwrap().game;
    revise(&mut m, [None, Some(0.5), None, None]);
    m.serve(&|_, _| None);
    assert_eq!(parameters(&m).second.wrn.confirmed, Some(0.5));
    m.command(MatchCommand::Intervention {
        op: MatchIntervention::Resign { side: Stone::Black },
    });
    m.serve(&|_, _| None);
    assert_eq!(m.phase(), Some(MatchPhase::BetweenGames));
    m.successor();
    assert_eq!(m.phase(), Some(MatchPhase::Playing));
    let view = m.wb.inspect().engine_match.unwrap();
    assert_ne!(view.game, previous);
    let p = view.parameters.unwrap();
    assert_eq!((p.second.wrn.confirmed, p.second.wrn.target), (Some(0.0), None));
    let stale = MatchCommand::ReviseParameters {
        game: previous,
        first_wrn: None,
        second_wrn: Some(1.0),
        first_pda: None,
        second_pda: None,
    };
    assert_eq!(refusal(m.command(stale)), "EngineGameParameters.notPlaying");
}

/// The 引擎参数 window during an engine match, until the batch gave its parameters back: the
/// toolbar choices and 自动加载 are saved, 修改 and the values are left alone and nothing is sent.
#[test]
fn the_engine_parameter_window_saves_only_preferences_until_the_batch_gave_its_values_back() {
    let mut m = Match::new();
    start_analysis(&mut m, OPENING, analysis());
    m.serve(&|_, _| None);
    let one = m.world.ids()[0];
    m.command(MatchCommand::Stop);
    m.serve(&move |i, c| (i == one && c == set(PDA, "1.5")).then(|| "HOLD".into()));
    assert_eq!(m.phase(), Some(MatchPhase::Ending), "the restore waits for 引擎 1");
    let row = KataParamEdit {
        enabled: true,
        value: Some(0.25),
        auto_load: true,
        edited: true,
        auto_load_edited: true,
    };
    let edit = KataEngineParamsEdit {
        pda: Some(row),
        wrn: row,
        show_pda: true,
        show_wrn: true,
    };
    let values = |m: &Match| {
        let e = &m.wb.config().engine;
        (e.kata_pda_enabled, e.kata_pda, e.kata_params.wrn_enabled, e.kata_params.wrn)
    };
    let before = values(&m);
    assert_eq!(m.wb.save_kata_engine_params(edit).unwrap(), None);
    assert_eq!(values(&m), before, "修改 and the values are left alone");
    let s = &m.wb.config().engine.kata_params;
    assert_eq!((s.auto_load_wrn, s.auto_load_wrn_value), (true, Some(0.25)));
    assert_eq!((s.show_pda, s.show_wrn), (true, true));
    let sent = m.serve(&|_, _| None);
    assert!(!sent.iter().any(|(_, c)| c.contains("0.25")), "{sent:?}");
}

#[test]
fn opening_values_are_saved_and_unreadable_ones_read_as_zero() {
    let dir = tempfile::tempdir().unwrap();
    let storage = || Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let workbench = || Workbench::new(storage(), None, Box::new(Launch(World::default())), vec![]);
    let saved = MatchOpeningParameters {
        first_wrn: 2.0,
        second_wrn: 0.5,
        first_pda: -3.0,
        second_pda: 1.25,
    };
    let mut wb = workbench();
    let mut settings = wb.config().matches.clone();
    settings.opening = saved;
    let session = MatchSessionSettings::default();
    wb.engine_match(MatchCommand::SetValues {
        settings: settings.clone(),
        session: session.clone(),
    })
    .unwrap();
    for bad in [
        MatchOpeningParameters {
            first_wrn: 2.5,
            ..saved
        },
        MatchOpeningParameters {
            second_wrn: -0.1,
            ..saved
        },
        MatchOpeningParameters {
            first_pda: 3.5,
            ..saved
        },
    ] {
        let refused = wb.engine_match(MatchCommand::SetValues {
            settings: MatchSettings {
                opening: bad,
                ..settings.clone()
            },
            session: session.clone(),
        });
        assert!(refused.is_err(), "{bad:?}");
    }
    drop(wb);
    assert_eq!(workbench().config().matches.opening, saved, "read back after a restart");

    let file = storage().config_file;
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    // Missing, unreadable and out-of-range values are 0; the old 禁用WRN plays no part.
    let zero = MatchOpeningParameters::default();
    for (game_wrn, opening, expected) in [
        (
            true,
            serde_json::json!({"firstWrn": "wide", "secondWrn": 2.5, "firstPda": -3.5, "secondPda": 2}),
            MatchOpeningParameters {
                second_pda: 2.0,
                ..zero
            },
        ),
        (false, serde_json::json!({"firstWrn": null}), zero),
        (true, serde_json::json!("not an object"), zero),
    ] {
        json["matches"]["opening"] = opening;
        json["game"]["disableWrn"] = serde_json::Value::Bool(game_wrn);
        std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
        let wb = workbench();
        assert_eq!(wb.config().matches.opening, expected, "{}", json["matches"]["opening"]);
        assert_eq!(
            wb.config().matches.komi,
            settings.komi,
            "the other values stay readable"
        );
    }
    json["matches"].as_object_mut().unwrap().remove("opening");
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
    assert_eq!(
        workbench().config().matches.opening,
        MatchOpeningParameters::default(),
        "a file from before has none"
    );
}

// 本局规则 choices: the Chinese rule sets of the rules panel (`RulePreset`) before Java's other
// `MatchRuleOption`s. Expected names are the English bundle's `MatchRuleOption` labels and
// `LizzieFrame.currentRules.*` short names.

/// What KataGo answers `kata-get-rules` with after `kata-set-rules <preset>`: the fields of its
/// official presets as Java `KataGoRules.officialPresets` lists them, written out here.
fn katago_preset(name: &str) -> Option<&'static str> {
    Some(match name {
        "chinese" => {
            r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":false,"whiteHandicapBonus":"N","friendlyPassOk":true}"#
        }
        "japanese" => {
            r#"{"ko":"SIMPLE","scoring":"TERRITORY","tax":"SEKI","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":false}"#
        }
        "stone-scoring" => {
            r#"{"ko":"SIMPLE","scoring":"AREA","tax":"ALL","suicide":false,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#
        }
        "aga" => {
            r#"{"ko":"SITUATIONAL","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":false,"whiteHandicapBonus":"N-1","friendlyPassOk":true}"#
        }
        "new-zealand" => {
            r#"{"ko":"SITUATIONAL","scoring":"AREA","tax":"NONE","suicide":true,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#
        }
        "tromp-taylor" => {
            r#"{"ko":"POSITIONAL","scoring":"AREA","tax":"NONE","suicide":true,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":false}"#
        }
        "goe" => {
            r#"{"ko":"POSITIONAL","scoring":"AREA","tax":"NONE","suicide":true,"hasButton":false,"whiteHandicapBonus":"0","friendlyPassOk":true}"#
        }
        _ => return None,
    })
}

/// The rules each 本局规则 entry gives the engines, as KataGo reports them: Java `RulePreset` for
/// the Chinese rule sets, `MatchRuleOption` for the others.
fn rules_of_choice(id: &str) -> serde_json::Value {
    let rules = match id {
        "CHINESE_2027" | "CHINESE_2027_DRAW_TO_WHITE" | "CHINESE" => katago_preset("chinese").unwrap(),
        "CHINESE_2027_DRAW_TO_FIRST_PASS" => {
            r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":true,"whiteHandicapBonus":"N","friendlyPassOk":true}"#
        }
        "CHINESE_ANCIENT" => katago_preset("stone-scoring").unwrap(),
        "JAPANESE_KOREAN" => katago_preset("japanese").unwrap(),
        "AGA_BGA" => katago_preset("aga").unwrap(),
        "NEW_ZEALAND" => katago_preset("new-zealand").unwrap(),
        "TROMP_TAYLOR" => katago_preset("tromp-taylor").unwrap(),
        "GOE" => katago_preset("goe").unwrap(),
        other => panic!("no rules written out for {other}"),
    };
    serde_json::from_str(rules).unwrap()
}

/// A `kata-set-rules` argument as KataGo takes it: a preset's fields or a JSON object.
fn rules_given(argument: &str) -> serde_json::Value {
    let json = katago_preset(argument).unwrap_or(argument);
    serde_json::from_str(json).unwrap_or_else(|_| panic!("not KataGo rules: {argument}"))
}

/// A started game with the 本局规则 entry `id`; the engines start with Japanese rules, take what
/// `kata-set-rules` gives them and report it.
fn start_with_rules_choice(m: &mut Match, id: &str, values: MatchSessionSettings) -> Vec<(InstanceId, String)> {
    let option = lizzie_core::engine::rules::MATCH_RULE_OPTIONS
        .iter()
        .find(|o| o.id == id)
        .unwrap();
    let result = m.command(MatchCommand::Start {
        settings: MatchSettings {
            rules: option.rules.into(),
            rules_choice: Some(id.into()),
            ..MatchSettings::default()
        },
        session: values,
        first_engine: 0,
        second_engine: 1,
    });
    assert_eq!(result, CommandResult::Done, "{id}");
    let kept = KeptRules::default();
    let sent = m.serve(&|i, command| match command.strip_prefix("kata-set-rules ") {
        // As KataGo, an engine given a preset reports it as its JSON fields.
        Some(rules) => kept.run(i, &format!("kata-set-rules {}", rules_given(rules))),
        None => kept.run(i, command),
    });
    assert_eq!(
        m.phase(),
        Some(MatchPhase::Playing),
        "{id}: both engines confirmed the rules"
    );
    // Besides each engine's overlay, the foreground engine is given its analysis target, which
    // during a game is the game's rules: the same rules, written as JSON.
    let target = rules_of_choice(id);
    for (_, command) in &sent {
        if let Some(rules) = command.strip_prefix("kata-set-rules ") {
            assert_eq!(rules_given(rules), target, "{id}: {command}");
        }
    }
    sent
}

/// The game's rules overlay each engine was sent: its first `kata-set-rules`.
fn rules_sent(sent: &[(InstanceId, String)]) -> Vec<String> {
    let mut engines = HashSet::new();
    sent.iter()
        .filter(|(i, c)| c.starts_with("kata-set-rules ") && engines.insert(*i))
        .map(|(_, c)| c["kata-set-rules ".len()..].to_string())
        .collect()
}

#[test]
fn the_first_pass_draw_rule_set_sends_the_button_and_is_named_by_its_short_name() {
    let button = r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":true,"whiteHandicapBonus":"N","friendlyPassOk":true}"#;
    let mut m = Match::with_locale(Some("en-US"));
    let sent = start_with_rules_choice(&mut m, "CHINESE_2027_DRAW_TO_FIRST_PASS", batch());
    let given = rules_sent(&sent);
    assert_eq!(given.len(), 2, "each engine is given the game's rules: {sent:?}");
    for rules in &given {
        assert_eq!(
            serde_json::from_str::<serde_json::Value>(rules).unwrap(),
            serde_json::from_str::<serde_json::Value>(button).unwrap()
        );
    }
    let name = "2027, tie to 1st pass";
    assert_eq!(caption(&m).as_deref(), Some(name));
    let fields = format!("{name} (ko=SIMPLE, scoring=AREA, tax=NONE, suicide=false, hasButton=true, whiteHandicapBonus=N, friendlyPassOk=true)");
    let expected = format!(
        "Target: {name}\nconfirmed\nBlack: First (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {fields}\nWhite: Second (/opt/katago gtp)\nStatus: confirmed\nEffective rules: {fields}"
    );
    assert_eq!(details(&m), Some(expected.clone()));
    assert_eq!(m.wb.inspect().engine_match.unwrap().rules_target_name, name);

    // The saved record's block and the batch summary use the same name.
    m.finish_turn("resign");
    let jobs = m.wb.take_background_saves();
    let sgf = jobs
        .iter()
        .find(|j| j.path.extension().is_some_and(|e| e == "sgf"))
        .unwrap();
    let record = lizzie_core::formats::parse(lizzie_core::record::RecordFormat::Sgf, &sgf.text, false).unwrap();
    assert!(
        record
            .node(ROOT)
            .comment
            .contains(&format!("Match rules\n{expected}\nEnd of match rules")),
        "{}",
        record.node(ROOT).comment
    );
    let summary = jobs
        .iter()
        .find(|j| j.path.extension().is_some_and(|e| e == "txt"))
        .unwrap();
    assert_eq!(
        summary.text.matches(&format!("\r\nRules: {name}")).count(),
        2,
        "{}",
        summary.text
    );

    let mut m = Match::with_locale(Some("zh-CN"));
    start_with_rules_choice(&mut m, "CHINESE_2027_DRAW_TO_FIRST_PASS", genmove());
    assert_eq!(caption(&m).as_deref(), Some("2027·平局先停胜"));
}

#[test]
fn the_other_chinese_rule_sets_send_the_chinese_rules_and_are_all_named_chinese() {
    for id in ["CHINESE_2027", "CHINESE_2027_DRAW_TO_WHITE", "CHINESE"] {
        let mut m = Match::with_locale(Some("en-US"));
        let sent = start_with_rules_choice(&mut m, id, genmove());
        assert_eq!(rules_sent(&sent), ["chinese", "chinese"], "{id}");
        assert_eq!(caption(&m).as_deref(), Some("Chinese"), "{id}");
        assert!(details(&m).unwrap().starts_with("Target: Chinese\nconfirmed\n"), "{id}");
        assert_eq!(
            m.wb.inspect().engine_match.unwrap().rules_target_name,
            "Chinese",
            "{id}"
        );
    }
}

#[test]
fn javas_other_rules_choices_are_sent_and_named_as_before() {
    for (id, preset, name) in [
        ("CHINESE_ANCIENT", "stone-scoring", "ChnAncient"),
        ("JAPANESE_KOREAN", "japanese", "Japanese / Korean"),
        ("AGA_BGA", "aga", "AGA / BGA"),
        ("NEW_ZEALAND", "new-zealand", "New Zealand"),
        ("TROMP_TAYLOR", "tromp-taylor", "Tromp-Taylor"),
        ("GOE", "goe", "Ing (GOE)"),
    ] {
        let mut m = Match::with_locale(Some("en-US"));
        let sent = start_with_rules_choice(&mut m, id, genmove());
        assert_eq!(rules_sent(&sent), [preset, preset], "{id}");
        assert_eq!(caption(&m).as_deref(), Some(name), "{id}");
        assert_eq!(m.wb.inspect().engine_match.unwrap().rules_target_name, name, "{id}");
    }
}

/// The entry is remembered with the start; the Chinese entries share their rules, so only the
/// remembered entry tells them apart, and one that does not name them is the 2002 rules.
#[test]
fn the_rules_entry_is_remembered_and_an_unreadable_one_is_the_2002_rules() {
    let dir = tempfile::tempdir().unwrap();
    let storage = || Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data"));
    let workbench = || Workbench::new(storage(), None, Box::new(Launch(World::default())), vec![]);
    let shown = |wb: &Workbench| wb.inspect().settings.matches.rules_choice;
    let mut wb = workbench();
    assert_eq!(
        shown(&wb).as_deref(),
        Some("CHINESE"),
        "nothing saved yet: Chinese rules, the 2002 entry"
    );
    for (rules, choice, expected) in [
        (
            "chinese",
            Some("CHINESE_2027_DRAW_TO_WHITE"),
            Some("CHINESE_2027_DRAW_TO_WHITE"),
        ),
        ("chinese", Some("CHINESE_2027"), Some("CHINESE_2027")),
        ("japanese", Some("JAPANESE_KOREAN"), Some("JAPANESE_KOREAN")),
        ("chinese", Some("JAPANESE_KOREAN"), Some("CHINESE")),
        ("chinese", Some("CHINESE_2028"), Some("CHINESE")),
        ("chinese", None, Some("CHINESE")),
        (
            r#"{"ko":"SIMPLE","scoring":"AREA","tax":"NONE","suicide":false,"hasButton":true,"whiteHandicapBonus":"N","friendlyPassOk":true}"#,
            None,
            Some("CHINESE_2027_DRAW_TO_FIRST_PASS"),
        ),
        (JAPANESE_RULES.replace("SEKI", "NONE").as_str(), None, None),
    ] {
        let settings = MatchSettings {
            rules: rules.into(),
            rules_choice: choice.map(String::from),
            ..wb.config().matches.clone()
        };
        wb.engine_match(MatchCommand::SetValues {
            settings,
            session: MatchSessionSettings::default(),
        })
        .unwrap();
        assert_eq!(shown(&wb).as_deref(), expected, "{rules} {choice:?}");
    }
    let settings = MatchSettings {
        rules: "chinese".into(),
        rules_choice: Some("CHINESE_2027_DRAW_TO_WHITE".into()),
        ..wb.config().matches.clone()
    };
    wb.engine_match(MatchCommand::SetValues {
        settings,
        session: MatchSessionSettings::default(),
    })
    .unwrap();
    drop(wb);
    assert_eq!(
        shown(&workbench()).as_deref(),
        Some("CHINESE_2027_DRAW_TO_WHITE"),
        "read back after a restart"
    );

    // A settings file whose entry is no text keeps everything else.
    let file = storage().config_file;
    let mut json: serde_json::Value = serde_json::from_slice(&std::fs::read(&file).unwrap()).unwrap();
    json["matches"]["rulesChoice"] = serde_json::json!(7);
    json["matches"]["komi"] = serde_json::json!(6.5);
    std::fs::write(&file, serde_json::to_vec(&json).unwrap()).unwrap();
    let wb = workbench();
    assert_eq!(wb.config().matches.komi, 6.5);
    assert_eq!(shown(&wb).as_deref(), Some("CHINESE"));
}
