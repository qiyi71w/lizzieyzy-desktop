//! Independent expectations: Java EngineGameBatchState, EngineGameModule and Leelaz at the
//! fixed migration baseline. Controlled processes exercise the same public Workbench interface.
use lizzie_core::api::*;
use lizzie_core::engine::gtp::LaunchSpec;
use lizzie_core::engine::profile::{EngineProfile, EngineStartup};
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
    fn launch(&self, id: InstanceId, _: &LaunchSpec) -> std::io::Result<Box<dyn EngineIo>> {
        self.0 .0.lock().unwrap().push(Process {
            id,
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
        let dir = tempfile::tempdir().unwrap();
        let world = World::default();
        let mut wb = Workbench::new(
            Storage::in_dirs(&dir.path().join("config"), &dir.path().join("data")),
            locale,
            Box::new(Launch(world.clone())),
            vec![],
        );
        wb.set_auto_quick_analyze(false).unwrap();
        wb.save_engine_settings(
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
            EngineStartup::Default,
        )
        .unwrap();
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
                    let text = match cmd.split(' ').next().unwrap() {
                        "name" => "KataGo",
                        "version" => "1.18.2",
                        "list_commands" => COMMANDS,
                        "kata-get-rules" => RULES,
                        "kata-get-param" => "1.5",
                        _ => "",
                    };
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
