//! Engine-versus-engine matches: one writer for acceptance, turns, retirement and batch succession.
//! Runtime reservations own processes; every document mutation uses apply_edit/edit_document.
mod saving;

use std::path::{Path, PathBuf};
use std::time::{Duration, Instant};

use super::{SaveJob, Workbench};
use crate::api::*;
use crate::engine::runtime::{GenmoveReply, InstanceId, LeaseFailure, Phase, RuntimeEvent};
use crate::record::{DocumentEdit, GameRecord, NodeId, NodeKind, Point, RecordFormat, Stone, ROOT};
use crate::settings::{MatchIdentity, MatchSettings, MatchTimeMode};

/// EngineManager.java:77–78,3385–3436: shared readiness/setup deadline.
const MATCH_STARTUP: Duration = Duration::from_secs(180);
/// EngineManager.restoreMatchRulesSide waits 30 seconds before reporting unrestored.
const MATCH_RETIRE: Duration = Duration::from_secs(30);

struct Turn {
    request: u64,
    game: u64,
    seat: u64,
    doc: u64,
    node: NodeId,
    stamp: u64,
    side: Stone,
    started: Instant,
}
struct Participant {
    index: u32,
    instance: InstanceId,
    reservation: u64,
    name: String,
    setup: bool,
    rules_verified: bool,
    original_rules: Option<crate::engine::rules::KataRules>,
    observed_rules: Option<crate::engine::rules::KataRules>,
    awaiting: Vec<u64>,
    captures: Vec<(u64, String)>,
    restore: Vec<String>,
    released: bool,
    resign_count: u32,
    seconds: f64,
    clock: Option<crate::play::EngineClock>,
    visits: f64,
    pda: Option<f64>,
    wrn: Option<f64>,
}
struct Run {
    id: u64,
    game: u64,
    doc: u64,
    rules_consent: bool,
    phase: MatchPhase,
    settings: MatchSettings,
    values: MatchSessionSettings,
    sides: [Participant; 2],
    first_is_black: bool,
    ordinal: u32,
    completed: u32,
    limit: u32,
    score: MatchScoreView,
    total_seconds: [f64; 2],
    previous: [Option<NodeAnalysisView>; 2],
    opening_scores: Vec<(u32, MatchScoreView)>,
    opening: Option<usize>,
    fixed: Option<(GameRecord, NodeId)>,
    turn: Option<Turn>,
    next_request: u64,
    manual: bool,
    play_now: bool,
    stop_batch: bool,
    image_pending: bool,
    next_game: Option<Instant>,
    started: Instant,
    setup_deadline: Option<Instant>,
    folder: PathBuf,
    timestamp: String,
    outcome: Option<MatchOutcomeView>,
    komi_pending: Option<f64>,
    komi_in_flight: Option<f64>,
    komi_awaiting: Vec<(u64, u64)>,
}
#[derive(Default)]
pub(super) struct MatchFlow {
    run: Option<Run>,
    next: u64,
    openings: Vec<(String, GameRecord)>,
    outcome: Option<MatchOutcomeView>,
    notices: Vec<MatchNotice>,
    image: Option<MatchWinrateImageRequest>,
    image_write: Option<MatchWinrateImageRequest>,
    jobs: Vec<SaveJob>,
    saves: Vec<u64>,
}
fn refused(key: &str) -> CommandResult {
    CommandResult::Refused {
        message_key: key.into(),
    }
}
fn side_index(first_black: bool, side: Stone) -> usize {
    usize::from((side == Stone::Black) != first_black)
}
fn safe(value: &str) -> String {
    value
        .chars()
        .map(|c| {
            if "/\\:*?|".contains(c) {
                '.'
            } else if "\"<>".contains(c) {
                '\''
            } else if c.is_control() {
                '_'
            } else {
                c
            }
        })
        .collect()
}

fn record_outcome(score: &mut MatchScoreView, end: MatchEnd, winner: Option<Stone>, first_black: bool) {
    match end {
        MatchEnd::DoublePass => score.double_passes += 1,
        MatchEnd::MaxMoves => score.max_moves += 1,
        MatchEnd::Resigned => {
            if let Some(winner) = winner {
                match (side_index(first_black, winner), winner) {
                    (0, Stone::Black) => score.first_black_wins += 1,
                    (0, Stone::White) => score.first_white_wins += 1,
                    (1, Stone::Black) => score.second_black_wins += 1,
                    _ => score.second_white_wins += 1,
                }
            }
        }
        _ => {}
    }
}
impl Workbench {
    pub(super) fn match_active(&self) -> bool {
        self.matches.run.is_some()
    }
    pub(super) fn match_analyse(&self) -> Option<bool> {
        self.matches.run.as_ref().map(|r| {
            r.phase == MatchPhase::Playing
                && r.values.kind == GameKind::Analysis
                && r.komi_pending.is_none()
                && r.komi_in_flight.is_none()
                && r.komi_awaiting.is_empty()
        })
    }
    fn match_side(&self) -> Stone {
        self.doc.record.node(self.cursor).to_play
    }
    pub(super) fn match_foreground_current(&self) -> bool {
        let Some(run) = &self.matches.run else { return true };
        if matches!(run.phase, MatchPhase::Ending | MatchPhase::BetweenGames) {
            return true;
        }
        let mover = run.sides[side_index(run.first_is_black, self.match_side())].instance;
        self.engines.foreground() == Some(mover)
    }
    pub(super) fn match_rules(&self) -> Option<crate::engine::rules::KataRules> {
        let r = self.matches.run.as_ref()?;
        if !matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused) {
            return None;
        }
        crate::engine::rules::KataRules::parse(&r.settings.rules)
    }
    pub(super) fn match_edit_refusal(&self, op: &EditCommand) -> Option<CommandResult> {
        let run = self.matches.run.as_ref()?;
        match op {
            EditCommand::Play { color, .. }
                if run.manual && run.phase == MatchPhase::Playing && color.is_none_or(|c| c == self.match_side()) =>
            {
                None
            }
            EditCommand::Pass if run.manual && run.phase == MatchPhase::Playing => None,
            EditCommand::SetComment { .. } | EditCommand::ToggleMark { .. } | EditCommand::ClearMarks => None,
            _ => Some(refused("LizzieFrame.engineGameStopFirstHint")),
        }
    }
    pub(super) fn match_gate(&self, command: &Command) -> Option<CommandResult> {
        if !self.match_active() {
            return None;
        }
        match command {
            Command::Match { .. } | Command::SetKomi { .. } => None,
            Command::ChooseAndOpenRecord
            | Command::OpenRecord { .. }
            | Command::PasteRecord { .. }
            | Command::Recovery {
                op: RecoveryCommand::LoadSlot { .. },
            } => Some(refused(super::play::OPEN_IN_GAME)),
            Command::Navigate { .. }
            | Command::GotoNode { .. }
            | Command::GotoMoveNumber { .. }
            | Command::JumpToProblemMove { .. }
            | Command::Hawkeye {
                op:
                    HawkeyeCommand::GotoRow { .. } | HawkeyeCommand::GotoMove { .. } | HawkeyeCommand::GotoMoveNumber { .. },
            } => Some(CommandResult::Unchanged),
            Command::Edit { op } => self.match_edit_refusal(op),
            Command::SendGtpCommand { .. } => Some(refused("GtpConsolePane.isEngineGame")),
            Command::Game { .. }
            | Command::SwitchEngine { .. }
            | Command::RestartEngine
            | Command::ShutdownEngines { .. }
            | Command::SaveEngineSettings { .. }
            | Command::SetKataRules { .. }
            | Command::ToggleRawNet
            | Command::StopRawNet
            | Command::StartFlashAnalysis { .. }
            | Command::RedoFlashAnalysis
            | Command::StartAutoAnalysis { .. }
            | Command::StartWholeGameAnalysis { .. }
            | Command::ResumeWholeGameAnalysis
            | Command::Region { .. }
            | Command::Tracking { .. }
            | Command::Batch { .. }
            | Command::Tuning {
                op: TuningCommand::StartBenchmark { .. },
            } => Some(refused("LizzieFrame.engineGameStopFirstHint")),
            _ => None,
        }
    }
    pub fn engine_match(&mut self, op: MatchCommand) -> Result<CommandResult, ApiError> {
        match op {
            MatchCommand::Start {
                mut settings,
                session,
                first_engine,
                second_engine,
                disable_wrn,
            } => {
                if self.match_active() || self.game_active() || self.play_analyse().is_some() {
                    return Ok(refused(super::play::PLAY_MODE));
                }
                if self.setup_mode() {
                    return Ok(refused("EngineManager.setupModeActive"));
                }
                self.validate_match(&settings, &session)?;
                if first_engine == second_engine {
                    return Ok(refused("EngineManager.engineGameSameEngine"));
                }
                let mut profiles = Vec::new();
                for index in [first_engine, second_engine] {
                    profiles.push(
                        self.config
                            .engine
                            .engines
                            .get(index as usize)
                            .cloned()
                            .ok_or_else(|| ApiError::new(ErrorKind::InvalidArgument, "match participant"))?,
                    );
                }
                settings.first = MatchIdentity {
                    commands: profiles[0].command.clone(),
                    name: profiles[0].name.clone(),
                };
                settings.second = MatchIdentity {
                    commands: profiles[1].command.clone(),
                    name: profiles[1].name.clone(),
                };
                if session.kind == GameKind::Analysis {
                    for (seconds, visits, first_visits) in [
                        (session.first_seconds, session.first_visits, session.first_move_visits),
                        (
                            session.second_seconds,
                            session.second_visits,
                            session.second_move_visits,
                        ),
                    ] {
                        if !(session.time_enabled && seconds > 0.0)
                            && !(session.visits_enabled && visits > 0)
                            && !(session.first_move_visits_enabled && first_visits > 0)
                        {
                            return Ok(refused("EngineManager.engineGameBlackSettingWrong"));
                        }
                    }
                }
                if settings.sgf_start && self.matches.openings.is_empty() {
                    return Ok(refused("NewEngineGameDialog.message"));
                }
                if settings.sgf_start && self.matches.openings.len() > 1 && !session.batch_enabled {
                    return Ok(refused("NewEngineGameDialog.multiSgfNotBatch"));
                }
                if settings.rules.trim().is_empty() {
                    settings.rules = self.match_settings_view().rules_prefill;
                }
                if self.quick_holds_foreground()
                    || self.flash_holds_foreground()
                    || self.whole_game_holds_foreground()
                    || self.batch_holds_foreground()
                    || self.auto_analysis_active()
                {
                    return Ok(refused(super::play::PLAY_MODE));
                }
                self.match_values(settings.clone(), session.clone(), disable_wrn)?;
                self.paused = true;
                self.start_komi = None;
                self.engines.stop_raw_net();
                self.engines.want_secondary(None, false);
                self.engines.want(None, false);
                let wanted = [&profiles[0].id, &profiles[1].id];
                for (id, profile, _) in self.engines.running() {
                    if !wanted.iter().any(|p| **p == profile) {
                        self.engines.terminate(id);
                    }
                }
                let mut seats = Vec::new();
                for (n, profile) in profiles.iter().enumerate() {
                    let id = match self.engines.instance_of(&profile.id) {
                        Some(id) => id,
                        None => match self.engines.start(profile, self.bootstrap(profile)) {
                            Ok(id) => id,
                            Err(_) => {
                                for p in &seats {
                                    let p: &Participant = p;
                                    self.engines.release_game(p.reservation, &[]);
                                }
                                return Ok(refused("EngineManager.engineGameStartFailed"));
                            }
                        },
                    };
                    let reservation = match self.engines.reserve_game_instance(id) {
                        Ok(r) => r,
                        Err(_) => {
                            for p in &seats {
                                let p: &Participant = p;
                                self.engines.release_game(p.reservation, &[]);
                            }
                            return Ok(refused("EngineManager.engineGameStartFailed"));
                        }
                    };
                    seats.push(Participant {
                        index: [first_engine, second_engine][n],
                        instance: id,
                        reservation,
                        name: profile.name.clone(),
                        setup: false,
                        rules_verified: false,
                        original_rules: None,
                        observed_rules: None,
                        awaiting: Vec::new(),
                        captures: Vec::new(),
                        restore: Vec::new(),
                        released: false,
                        resign_count: 0,
                        seconds: 0.0,
                        visits: 0.0,
                        clock: None,
                        pda: None,
                        wrn: None,
                    });
                }
                let fixed = session
                    .continue_position
                    .then(|| ((*self.doc.record).clone(), self.cursor));
                let timestamp = chrono::Local::now().format("%Y%m%d%H%M%S").to_string();
                let mut folder = self
                    .storage
                    .save_dir
                    .parent()
                    .unwrap_or(Path::new("."))
                    .join("EngineGames");
                if session.batch_enabled {
                    folder = folder.join(if session.batch_name.is_empty() {
                        safe(&format!("{}_VS_{}_{timestamp}", profiles[0].name, profiles[1].name))
                    } else {
                        safe(&session.batch_name)
                    });
                }
                self.matches.next += 1;
                let id = self.matches.next;
                self.matches.run = Some(Run {
                    id,
                    game: id,
                    doc: self.doc.id,
                    phase: MatchPhase::Starting,
                    rules_consent: false,
                    settings,
                    values: session.clone(),
                    sides: seats.try_into().ok().unwrap(),
                    first_is_black: true,
                    ordinal: 1,
                    completed: 0,
                    limit: if session.batch_enabled {
                        session.batch_limit.max(1)
                    } else {
                        1
                    },
                    score: MatchScoreView::default(),
                    total_seconds: [0.0, 0.0],
                    previous: [None, None],
                    opening_scores: Vec::new(),
                    opening: None,
                    fixed,
                    turn: None,
                    next_request: 1,
                    manual: false,
                    play_now: false,
                    stop_batch: false,
                    image_pending: false,
                    next_game: None,
                    started: Instant::now(),
                    setup_deadline: Some(Instant::now() + MATCH_STARTUP),
                    folder,
                    timestamp,
                    outcome: None,
                    komi_pending: None,
                    komi_in_flight: None,
                    komi_awaiting: Vec::new(),
                });
                self.persisted.matches.batch_name.clear();
                self.match_prepare_board();
                self.match_setup();
                Ok(CommandResult::Done)
            }
            MatchCommand::StartToolbar => {
                let view = self.match_settings_view();
                self.engine_match(MatchCommand::Start {
                    settings: view.settings,
                    session: view.session,
                    first_engine: view.first_engine,
                    second_engine: view.second_engine,
                    disable_wrn: view.disable_wrn,
                })
            }
            MatchCommand::SetValues {
                settings,
                session,
                disable_wrn,
            } => {
                self.validate_match(&settings, &session)?;
                self.match_values(settings, session, disable_wrn)?;
                Ok(CommandResult::Done)
            }
            MatchCommand::Stop => {
                if let Some(r) = &mut self.matches.run {
                    r.stop_batch = true;
                }
                self.match_end(MatchEnd::UserStopped, None);
                Ok(CommandResult::Done)
            }
            MatchCommand::Pause => {
                let Some(r) = self.matches.run.as_mut() else {
                    return Ok(CommandResult::Unchanged);
                };
                match r.phase {
                    MatchPhase::Playing => {
                        r.phase = MatchPhase::Paused;
                    }
                    MatchPhase::Paused
                        if r.turn.is_none()
                            && r.komi_pending.is_none()
                            && r.komi_in_flight.is_none()
                            && r.komi_awaiting.is_empty() =>
                    {
                        r.phase = MatchPhase::Playing;
                    }
                    MatchPhase::Paused if r.turn.is_some() => return Ok(refused("BottomToolbar.genmoveStopHint")),
                    _ => return Ok(CommandResult::Unchanged),
                }
                self.match_dispatch();
                self.sync_engine();
                Ok(CommandResult::Done)
            }
            MatchCommand::ReviseBatchLimit { limit } => {
                if let Some(r) = &mut self.matches.run {
                    r.limit = limit;
                }
                self.persisted.matches.batch_limit = limit;
                self.save_persisted()?;
                self.match_retirement();
                Ok(CommandResult::Done)
            }
            MatchCommand::Intervention { op } => {
                let Some(r) = self
                    .matches
                    .run
                    .as_mut()
                    .filter(|r| matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused))
                else {
                    return Ok(CommandResult::Unchanged);
                };
                match op {
                    MatchIntervention::Resign { side } => self.match_end(MatchEnd::Resigned, Some(side.opposite())),
                    MatchIntervention::ToggleManual if r.values.kind == GameKind::Analysis => {
                        r.manual = !r.manual;
                    }
                    MatchIntervention::PlayNow if r.values.kind == GameKind::Analysis => {
                        r.play_now = true;
                        if let Some(result) = self.store.get(&self.doc.record, self.cursor).cloned() {
                            self.match_analysis(&result, Duration::ZERO);
                        }
                    }
                    _ => return Ok(CommandResult::Unchanged),
                }
                Ok(CommandResult::Done)
            }
            MatchCommand::ReplaceOpenings { files } => {
                if self.match_active() {
                    return Ok(refused("LizzieFrame.engineGameStopFirstHint"));
                }
                let mut openings = Vec::new();
                for file in files {
                    let record = crate::formats::parse(
                        RecordFormat::Sgf,
                        &crate::formats::decode(&file.bytes),
                        self.config.edit.no_capture,
                    )
                    .map_err(|e| ApiError::new(ErrorKind::InvalidArgument, format!("{}: {e}", file.path)))?;
                    openings.push((file.path, record));
                }
                self.matches.openings = openings;
                Ok(CommandResult::Done)
            }
            MatchCommand::ConfirmRules { game, allow } => {
                let Some(run) = self
                    .matches
                    .run
                    .as_mut()
                    .filter(|r| r.game == game && r.phase == MatchPhase::Starting)
                else {
                    return Ok(CommandResult::Unchanged);
                };
                if allow {
                    run.rules_consent = true;
                    self.match_setup();
                } else {
                    self.match_end(MatchEnd::Retired, None);
                }
                Ok(CommandResult::Done)
            }
            MatchCommand::WinrateImageDone { game, error } => {
                let Some(r) = self.matches.run.as_mut().filter(|r| r.game == game && r.image_pending) else {
                    return Ok(CommandResult::Unchanged);
                };
                r.image_pending = false;
                self.matches.image_write = None;
                if let Some(error) = error {
                    self.log(true, format!("saving match winrate image failed: {error}"));
                }
                self.match_retirement();
                Ok(CommandResult::Done)
            }
        }
    }
    fn validate_match(&self, s: &MatchSettings, v: &MatchSessionSettings) -> Result<(), ApiError> {
        if ![
            s.komi,
            s.first_resign.winrate,
            s.second_resign.winrate,
            s.random_visits_percent,
            v.first_seconds,
            v.second_seconds,
            v.random_winrate_diff,
        ]
        .iter()
        .all(|v| v.is_finite())
            || ![0, 2, 3, 4, 5, 6, 7, 8, 9].contains(&s.handicap)
            || (s.handicap > 0 && (self.doc.record.size.width != 19 || self.doc.record.size.height != 19))
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "match settings"));
        }
        if !s.rules.trim().is_empty()
            && crate::engine::rules::KataRules::parse(&s.rules).is_none_or(|r| !r.has_required_fields())
        {
            return Err(ApiError::new(
                ErrorKind::InvalidArgument,
                "NewEngineGameDialog.matchRulesCorrupt",
            ));
        }
        // Java joins the one-shot folder verbatim. A dot/separator-only component can escape
        // EngineGames (or collapse to a parent on Windows) rather than name a batch folder.
        if !v.batch_name.is_empty()
            && v.batch_name
                .chars()
                .all(|c| matches!(c, '.' | '/' | '\\' | ':' | '*' | '?' | '|') || c.is_whitespace())
        {
            return Err(ApiError::new(ErrorKind::InvalidArgument, "batch folder name"));
        }
        for c in [&s.first_time_command, &s.second_time_command] {
            if c.contains(['\n', '\r']) {
                return Err(ApiError::new(ErrorKind::InvalidArgument, "time command"));
            }
        }
        Ok(())
    }
    fn match_values(
        &mut self,
        settings: MatchSettings,
        session: MatchSessionSettings,
        disable_wrn: bool,
    ) -> Result<(), ApiError> {
        self.commit_config(|c| {
            c.matches = settings;
            c.game.disable_wrn = disable_wrn;
        })?;
        self.persisted.matches = session;
        self.save_persisted()
    }
    pub(super) fn match_settings_view(&self) -> MatchSettingsView {
        let resolve = |identity: &MatchIdentity| -> u32 {
            let profiles = &self.config.engine.engines;
            profiles
                .iter()
                .position(|p| p.command == identity.commands && p.name == identity.name)
                .or_else(|| profiles.iter().position(|p| p.command == identity.commands))
                .unwrap_or(0) as u32
        };
        use crate::engine::rules::KataRules;
        let stored = KataRules::parse(&self.config.matches.rules).filter(KataRules::has_required_fields);
        let rules_corrupt = !self.config.matches.rules.trim().is_empty() && stored.is_none();
        let prefill = stored
            .or_else(|| {
                self.config
                    .engine
                    .auto_load_kata_rules
                    .then(|| KataRules::parse(&self.config.engine.kata_rules))
                    .flatten()
            })
            .unwrap_or_else(|| KataRules::preset("chinese").unwrap());
        let rules_prefill = prefill.to_gtp_argument();
        let rules_choice = [
            "chinese",
            "stone-scoring",
            "japanese",
            "aga",
            "new-zealand",
            "tromp-taylor",
            "goe",
        ]
        .iter()
        .find(|name| KataRules::preset(name).is_some_and(|p| p == prefill))
        .map(|s| s.to_string());
        MatchSettingsView {
            rules_prefill,
            rules_choice,
            rules_corrupt,
            settings: self.config.matches.clone(),
            session: self.persisted.matches.clone(),
            disable_wrn: self.config.game.disable_wrn,
            first_engine: resolve(&self.config.matches.first),
            second_engine: resolve(&self.config.matches.second),
            openings: self.matches.openings.iter().map(|(path, _)| path.clone()).collect(),
        }
    }
    fn match_prepare_board(&mut self) {
        let r = self.matches.run.as_ref().unwrap();
        let fixed = r.fixed.clone();
        let opening = if r.settings.sgf_start {
            Some(if r.settings.sgf_random {
                self.match_random(self.matches.openings.len())
            } else {
                (r.ordinal as usize - 1) % self.matches.openings.len()
            })
        } else {
            None
        };
        let source = opening
            .map(|i| {
                let record = self.matches.openings[i].1.clone();
                let end = crate::browse::navigate(&record, ROOT, crate::browse::Navigation::Last);
                (record, end)
            })
            .or(fixed);
        let black_index = r.sides[side_index(r.first_is_black, Stone::Black)].index;
        let black = &self.config.engine.engines[black_index as usize];
        let empty_size = crate::record::BoardSize {
            width: black.width,
            height: black.height,
        };
        let size = source.as_ref().map_or(empty_size, |(r, _)| r.size);
        self.reopen_empty_board(size);
        if let Some((rec, end)) = source {
            let pos = rec.position(ROOT);
            for (n, stone) in pos.stones.iter().enumerate() {
                if let Some(color) = stone {
                    let _ = self.apply_edit(DocumentEdit::SetupStone {
                        point: Point {
                            x: (n % size.width as usize) as u16,
                            y: (n / size.width as usize) as u16,
                        },
                        stone: Some(*color),
                    });
                }
            }
            let _ = self.apply_edit(DocumentEdit::SetupSideToPlay {
                color: rec.node(ROOT).to_play,
            });
            for &node in rec.path_to(end).iter().skip(1) {
                let edit = match rec.node(node).kind {
                    NodeKind::Move { color, point } => DocumentEdit::Play {
                        at: self.cursor,
                        color,
                        point,
                        alternate: false,
                        rules: self.play_rules(),
                    },
                    NodeKind::Pass { color } => DocumentEdit::Pass { at: self.cursor, color },
                    _ => continue,
                };
                if let Some(node) = self.apply_edit(edit).ok().and_then(|a| a.node) {
                    self.cursor = node;
                }
            }
        } else {
            let handicap = self.matches.run.as_ref().unwrap().settings.handicap;
            if handicap >= 2 {
                // Java Board.setupFixedHandicap's standard 19x19 star points.
                let points = [
                    (15, 3),
                    (3, 15),
                    (15, 15),
                    (3, 3),
                    (3, 9),
                    (15, 9),
                    (9, 3),
                    (9, 15),
                    (9, 9),
                ];
                let mut chosen: Vec<_> = points[..handicap.min(9) as usize].to_vec();
                if handicap == 5 {
                    chosen[4] = (9, 9);
                }
                if handicap == 7 {
                    chosen[6] = (9, 9);
                }
                for (x, y) in chosen {
                    let _ = self.apply_edit(DocumentEdit::SetupStone {
                        point: Point { x, y },
                        stone: Some(Stone::Black),
                    });
                }
                let _ = self.apply_edit(DocumentEdit::SetupSideToPlay { color: Stone::White });
                let _ = self.apply_edit(DocumentEdit::SetHandicap { handicap });
            }
        }
        let r = self.matches.run.as_mut().unwrap();
        r.doc = self.doc.id;
        r.opening = opening;
        let komi = r.settings.komi;
        let black = side_index(r.first_is_black, Stone::Black);
        let names = (r.sides[black].name.clone(), r.sides[1 - black].name.clone());
        self.komi_changed = true;
        self.apply_komi(komi);
        let _ = self.edit_document(DocumentEdit::SetPlayers {
            black: names.0,
            white: names.1,
        });
        self.sounds.clear();
        self.match_foreground();
        self.sync_engine();
    }
    fn match_random(&self, count: usize) -> usize {
        use std::hash::{BuildHasher, Hasher};
        let hash = std::collections::hash_map::RandomState::new().build_hasher().finish();
        (hash % count.max(1) as u64) as usize
    }
    fn match_foreground(&mut self) {
        let Some(r) = &self.matches.run else { return };
        let n = side_index(r.first_is_black, self.match_side());
        self.engines.want(None, false);
        self.engines.set_foreground(Some(r.sides[n].instance), true);
    }
    fn match_setup(&mut self) {
        let Some(r) = self.matches.run.as_ref().filter(|r| r.phase == MatchPhase::Starting) else {
            return;
        };
        let settings = r.settings.clone();
        let values = r.values.clone();
        // EngineManager.prepareMatchRulesForEngineGame verifies each overlay, or requires consent.
        for n in 0..2 {
            let r = self.matches.run.as_ref().unwrap();
            let p = &r.sides[n];
            if p.setup || self.engines.phase(p.instance) != Some(Phase::Ready) {
                continue;
            }
            let (seat, id) = (p.reservation, p.instance);
            let mut captures = Vec::new();
            let mut awaiting = Vec::new();
            let mut commands = Vec::new();
            let can_query = self.engines.has_command(id, "kata-get-rules");
            let can_set = self.engines.has_command(id, "kata-set-rules");
            if can_query {
                if let Some(command) = self.engines.game_send(seat, "kata-get-rules") {
                    captures.push((command, "rules".to_string()));
                    awaiting.push(command);
                }
                if can_set && !settings.rules.is_empty() {
                    commands.push(format!("kata-set-rules {}", settings.rules));
                }
            }
            if self.engines.is_katago(id) && self.engines.has_command(id, "kata-get-param") {
                for (parameter, role) in [
                    ("playoutDoublingAdvantage", "snapshotPda"),
                    ("analysisWideRootNoise", "snapshotWrn"),
                ] {
                    if role == "snapshotWrn" && values.kind == GameKind::Analysis && self.config.game.disable_wrn {
                        continue;
                    }
                    if let Some(command) = self.engines.game_send(seat, &format!("kata-get-param {parameter}")) {
                        captures.push((command, role.into()));
                        awaiting.push(command);
                    }
                }
            }
            if values.kind == GameKind::Genmove {
                let mode = if n == 0 {
                    settings.first_time_mode
                } else {
                    settings.second_time_mode
                };
                match mode {
                    MatchTimeMode::Fixed if values.time_enabled => {
                        let seconds = if n == 0 {
                            values.first_seconds
                        } else {
                            values.second_seconds
                        };
                        if seconds > 0.0 {
                            commands.push(format!("time_settings 0 {seconds} 1"));
                        }
                    }
                    MatchTimeMode::RawAdvanced => commands.push(if n == 0 {
                        settings.first_time_command.clone()
                    } else {
                        settings.second_time_command.clone()
                    }),
                    _ => {}
                }
            } else if self.config.game.disable_wrn && self.engines.is_katago(id) {
                if self.engines.has_command(id, "kata-get-param") {
                    if let Some(command) = self.engines.game_send(seat, "kata-get-param analysisWideRootNoise") {
                        captures.push((command, "analysisWideRootNoise".into()));
                        awaiting.push(command);
                    }
                }
                commands.push("kata-set-param analysisWideRootNoise 0".into());
            }
            if self.engines.has_command(id, "clear_cache") {
                commands.push("clear_cache".into());
            }
            commands.push("name".into());
            for c in commands {
                if let Some(command) = self.engines.game_send(seat, &c) {
                    awaiting.push(command);
                }
            }
            if can_query && can_set {
                if let Some(command) = self.engines.game_send(seat, "kata-get-rules") {
                    captures.push((command, "verifyRules".into()));
                    awaiting.push(command);
                }
            }
            let p = &mut self.matches.run.as_mut().unwrap().sides[n];
            p.setup = true;
            p.awaiting = awaiting;
            p.captures = captures;
            let mode = if n == 0 {
                settings.first_time_mode
            } else {
                settings.second_time_mode
            };
            let command = if n == 0 {
                &settings.first_time_command
            } else {
                &settings.second_time_command
            };
            let black = (n == 0) == self.matches.run.as_ref().unwrap().first_is_black;
            self.matches.run.as_mut().unwrap().sides[n].clock = if mode == MatchTimeMode::RawAdvanced {
                crate::play::EngineClock::parse(command, black)
            } else {
                None
            };
        }
        if self
            .matches
            .run
            .as_ref()
            .is_some_and(|r| r.sides.iter().all(|p| p.setup && p.awaiting.is_empty()))
        {
            let r = self.matches.run.as_mut().unwrap();
            if !r.rules_consent && r.sides.iter().any(|p| !p.rules_verified) {
                return;
            }
            r.phase = MatchPhase::Playing;
            r.setup_deadline = None;
            r.started = Instant::now();
            self.sync_engine();
            self.match_dispatch();
        }
    }
    fn match_dispatch(&mut self) {
        // EngineManager.pauseEngineGame leaves an unsettled Genmove alone. The next attempted
        // turn records its pending side and stops both participants (Leelaz.genmoveForPk).
        if let Some(run) = self
            .matches
            .run
            .as_ref()
            .filter(|r| r.phase == MatchPhase::Paused && r.turn.is_none())
        {
            for participant in &run.sides {
                let command = if self.engines.is_katago(participant.instance) {
                    "stop"
                } else {
                    "name"
                };
                self.engines.game_send(participant.reservation, command);
            }
            return;
        }
        let side = self.match_side();
        let target = self.analysis_target();
        let Some(r) = self.matches.run.as_mut().filter(|r| {
            r.phase == MatchPhase::Playing
                && r.turn.is_none()
                && r.komi_pending.is_none()
                && r.komi_in_flight.is_none()
                && r.komi_awaiting.is_empty()
        }) else {
            return;
        };
        if r.values.kind != GameKind::Genmove {
            return;
        }
        let participant = &mut r.sides[side_index(r.first_is_black, side)];
        let seat = participant.reservation;
        let time_left = participant.clock.as_mut().and_then(crate::play::EngineClock::claim);
        let request = r.next_request;
        r.next_request += 1;
        let turn = Turn {
            request,
            game: r.game,
            seat,
            doc: r.doc,
            node: target.node,
            stamp: target.record.node(target.node).stamp,
            side,
            started: Instant::now(),
        };
        if self
            .engines
            .genmove(seat, request, target, side, time_left, !r.settings.ponder)
        {
            r.turn = Some(turn);
        } else {
            self.match_end(MatchEnd::Retired, None);
        }
    }
    pub(super) fn match_event(&mut self, event: RuntimeEvent) -> Option<RuntimeEvent> {
        let owned = match &event {
            RuntimeEvent::GameReply { game, .. }
            | RuntimeEvent::GameReleased { game, .. }
            | RuntimeEvent::Genmove { game, .. } => self
                .matches
                .run
                .as_ref()
                .is_some_and(|r| r.sides.iter().any(|p| p.reservation == *game)),
            _ => false,
        };
        match event {
            RuntimeEvent::Ready { instance } => {
                self.match_setup();
                Some(RuntimeEvent::Ready { instance })
            }
            RuntimeEvent::GameReply {
                game,
                command,
                ok,
                text,
            } if owned => {
                self.match_reply(game, command, ok, text);
                None
            }
            RuntimeEvent::Genmove {
                game,
                request,
                doc,
                node,
                stamp,
                reply,
            } if owned => {
                self.match_genmove(game, request, doc, node, stamp, reply);
                self.engines.game_move_dropped(game, request);
                None
            }
            RuntimeEvent::GameReleased { game, failure } if owned => {
                let active = self
                    .matches
                    .run
                    .as_ref()
                    .is_some_and(|r| !matches!(r.phase, MatchPhase::Ending | MatchPhase::BetweenGames));
                let r = self.matches.run.as_mut().unwrap();
                if let Some(p) = r.sides.iter_mut().find(|p| p.reservation == game) {
                    p.released = true;
                }
                if active {
                    if self
                        .matches
                        .run
                        .as_ref()
                        .is_some_and(|r| r.phase == MatchPhase::Starting)
                    {
                        self.matches.notices.push(MatchNotice {
                            message_key: "EngineManager.engineGameStartFailed".into(),
                            arguments: Vec::new(),
                        });
                    }
                    self.match_end(MatchEnd::Retired, None);
                }
                if let Some(failure) = failure {
                    let reason = match failure {
                        LeaseFailure::TransportClosed => UnrestoredReason::Stop,
                        _ => UnrestoredReason::Restore,
                    };
                    let r = self.matches.run.as_mut()?;
                    r.stop_batch = true;
                    if let Some(outcome) = &mut r.outcome {
                        outcome.unrestored.push(reason);
                    }
                }
                self.match_retirement();
                None
            }
            event => Some(event),
        }
    }
    fn match_reply(&mut self, seat: u64, command: u64, ok: bool, text: String) {
        let Some(r) = &mut self.matches.run else { return };
        if let Some(at) = r.komi_awaiting.iter().position(|c| *c == (seat, command)) {
            r.komi_awaiting.remove(at);
            if !ok {
                self.match_end(MatchEnd::Retired, None);
                // EngineManager.java:7728–7756 retires both possibly modified participants.
                let instances = self
                    .matches
                    .run
                    .as_ref()
                    .map(|run| run.sides.each_ref().map(|p| p.instance));
                if let Some(instances) = instances {
                    for instance in instances {
                        self.engines.terminate(instance);
                    }
                    self.drain_engine_events();
                }
                return;
            }
            if r.komi_awaiting.is_empty() {
                if let Some(komi) = r.komi_in_flight.take() {
                    self.apply_komi(komi);
                }
                self.match_apply_pending_komi();
                self.sync_engine();
                self.match_dispatch();
            }
            return;
        }
        let Some(p) = r.sides.iter_mut().find(|p| p.reservation == seat) else {
            return;
        };
        if let Some(at) = p.awaiting.iter().position(|c| *c == command) {
            p.awaiting.remove(at);
        }
        let mut snapshot = false;
        let mut snapshot_failed = false;
        if let Some(at) = p.captures.iter().position(|(c, _)| *c == command) {
            let (_, name) = p.captures.remove(at);
            if name == "rules" || name == "verifyRules" {
                let actual = ok
                    .then(|| crate::engine::rules::KataRules::parse(&text))
                    .flatten()
                    .filter(crate::engine::rules::KataRules::has_required_fields);
                let target = crate::engine::rules::KataRules::parse(&r.settings.rules);
                if actual.is_none()
                    || (name == "verifyRules" && actual != target)
                    || (name == "rules" && !self.engines.has_command(p.instance, "kata-set-rules") && actual != target)
                {
                    self.matches.notices.push(MatchNotice {
                        message_key: "EngineManager.engineGameStartFailed".into(),
                        arguments: Vec::new(),
                    });
                    self.match_end(MatchEnd::Retired, None);
                    return;
                }
                p.observed_rules = actual.clone();
                if name == "rules" {
                    p.original_rules = actual;
                }
                if name == "rules" && !self.engines.has_command(p.instance, "kata-set-rules") {
                    p.rules_verified = true;
                    self.match_setup();
                    return;
                }
            }
            if matches!(name.as_str(), "snapshotPda" | "snapshotWrn") {
                snapshot = true;
                let value = ok
                    .then(|| text.trim().parse::<f64>().ok())
                    .flatten()
                    .filter(|value| value.is_finite());
                snapshot_failed = value.is_none();
                if name == "snapshotPda" {
                    p.pda = value;
                } else {
                    p.wrn = value;
                }
            } else if name == "verifyRules" {
                if r.phase == MatchPhase::Ending {
                    self.match_release();
                    return;
                }
                let target = crate::engine::rules::KataRules::parse(&r.settings.rules);
                let actual = crate::engine::rules::KataRules::parse(&text);
                if !ok || target.is_none() || actual != target {
                    self.matches.notices.push(MatchNotice {
                        message_key: "EngineManager.engineGameStartFailed".into(),
                        arguments: Vec::new(),
                    });
                    self.match_end(MatchEnd::Retired, None);
                    return;
                }
                p.rules_verified = true;
            } else if ok {
                if name == "analysisWideRootNoise" {
                    p.wrn = Some(0.0);
                }
                p.restore.push(if name == "rules" {
                    format!("kata-set-rules {}", text.trim())
                } else {
                    format!("kata-set-param {name} {}", text.trim())
                });
            }
        }
        let ending = r.phase == MatchPhase::Ending;
        if !ok && !snapshot && !ending {
            self.match_end(MatchEnd::Retired, None);
            return;
        }
        if snapshot_failed {
            self.log(
                true,
                "engine does not report the match comment parameter; omitting its unknown value".into(),
            );
        }
        if ending {
            self.match_release();
        } else {
            self.match_setup();
        }
    }
    fn match_genmove(&mut self, seat: u64, request: u64, doc: u64, node: NodeId, stamp: u64, reply: GenmoveReply) {
        let Some(r) = &mut self.matches.run else { return };
        let Some(turn) = r.turn.take_if(|t| t.seat == seat && t.request == request) else {
            return;
        };
        if !matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused)
            || turn.game != r.game
            || turn.doc != doc
            || doc != self.doc.id
            || node != self.cursor
            || node != turn.node
            || stamp != turn.stamp
            || self.doc.record.node(node).stamp != stamp
        {
            return;
        }
        let n = side_index(r.first_is_black, turn.side);
        r.sides[n].seconds += turn.started.elapsed().as_secs_f64();
        if let Some(clock) = &mut r.sides[n].clock {
            clock.elapse((turn.started.elapsed().as_millis() / 10) as u64);
        }
        let evaluation = self.store.get(&self.doc.record, node).cloned();
        if let Some(evaluation) = evaluation {
            self.matches.run.as_mut().unwrap().sides[n].visits += evaluation.visits as f64;
            self.match_record_analysis(&evaluation, turn.started.elapsed().as_secs_f64());
        }
        // Leelaz.parseLineForGenmovePk checks resign before the move-count/full-board guards.
        if matches!(reply, GenmoveReply::Resign) {
            self.match_end(MatchEnd::Resigned, Some(turn.side.opposite()));
            return;
        }
        let board_full = self.match_board_full();
        if self.doc.record.node(node).move_number > self.match_max_moves() || board_full {
            self.match_end(MatchEnd::MaxMoves, None);
            return;
        }
        match reply {
            GenmoveReply::Move(point) => self.match_commit(Some(point), turn.side, Some((seat, request))),
            GenmoveReply::Pass => self.match_commit(None, turn.side, Some((seat, request))),
            GenmoveReply::Resign => self.match_end(MatchEnd::Resigned, Some(turn.side.opposite())),
            GenmoveReply::Failed(_) => self.match_end(MatchEnd::Retired, None),
        }
        self.match_apply_pending_komi();
    }
    fn match_max_moves(&self) -> u32 {
        let r = self.matches.run.as_ref().unwrap();
        if r.values.max_moves_enabled && r.values.max_moves > 0 {
            r.values.max_moves
        } else {
            u32::from(self.doc.record.size.width) * u32::from(self.doc.record.size.height) * 2
        }
    }

    /// EngineManager.java:10250–10292: full-board termination is no-capture only.
    fn match_board_full(&self) -> bool {
        self.config.edit.no_capture && self.doc.record.position(self.cursor).stones.iter().all(Option::is_some)
    }
    fn match_commit(&mut self, point: Option<Point>, side: Stone, request: Option<(u64, u64)>) {
        let double_pass = point.is_none() && matches!(self.doc.record.node(self.cursor).kind, NodeKind::Pass { .. });
        let edit = match point {
            Some(point) => DocumentEdit::Play {
                at: self.cursor,
                color: side,
                point,
                alternate: false,
                rules: self.play_rules(),
            },
            None => DocumentEdit::Pass {
                at: self.cursor,
                color: side,
            },
        };
        let Some(node) = self.apply_edit(edit).ok().and_then(|a| a.node) else {
            self.match_end(MatchEnd::Retired, None);
            return;
        };
        self.cursor = node;
        self.engines.want(None, false);
        if let Some((seat, request)) = request {
            self.engines
                .game_move_committed(seat, request, self.doc.record.node(node).stamp);
        }
        if self.config.ui.play_sound {
            self.sounds.push(super::step_sound(&self.doc.record, node));
        }
        if self.match_board_full() {
            self.match_end(MatchEnd::MaxMoves, None);
            return;
        }
        if double_pass {
            self.match_end(MatchEnd::DoublePass, None);
            return;
        }
        self.match_foreground();
        self.sync_engine();
        self.match_dispatch();
    }
    pub(super) fn match_after_manual(&mut self) {
        let is_pass = matches!(self.doc.record.node(self.cursor).kind, NodeKind::Pass { .. });
        let double = is_pass
            && self
                .doc
                .record
                .parent(self.cursor)
                .is_some_and(|p| matches!(self.doc.record.node(p).kind, NodeKind::Pass { .. }));
        if self.match_board_full() {
            self.match_end(MatchEnd::MaxMoves, None);
        } else if double {
            self.match_end(MatchEnd::DoublePass, None);
        } else {
            self.match_foreground();
            self.sync_engine();
        }
    }
    pub(super) fn match_wants_analysis(&self, node: NodeId) -> bool {
        node == self.cursor
            && self.matches.run.as_ref().is_some_and(|r| {
                r.values.kind == GameKind::Analysis
                    && r.phase == MatchPhase::Playing
                    && r.komi_pending.is_none()
                    && r.komi_in_flight.is_none()
                    && r.komi_awaiting.is_empty()
            })
    }
    pub(super) fn match_analysis(&mut self, result: &NodeAnalysisView, elapsed: Duration) {
        if !self.match_wants_analysis(result.node) {
            return;
        }
        let moves = self.doc.record.node(self.cursor).move_number;
        if moves > self.match_max_moves() {
            self.match_end(MatchEnd::MaxMoves, None);
            return;
        }
        let Some(best) = result.candidates.first() else { return };
        let side = self.match_side();
        let r = self.matches.run.as_mut().unwrap();
        let n = side_index(r.first_is_black, side);
        let v = &r.values;
        let seconds = if n == 0 { v.first_seconds } else { v.second_seconds };
        let visits = if n == 0 { v.first_visits } else { v.second_visits };
        let first = if n == 0 {
            v.first_move_visits
        } else {
            v.second_move_visits
        };
        if !(r.play_now
            || (v.time_enabled && seconds > 0.0 && elapsed.as_secs_f64() >= seconds)
            || (v.visits_enabled && visits > 0 && result.visits >= u64::from(visits))
            || (v.first_move_visits_enabled && first > 0 && best.visits >= u64::from(first)))
        {
            return;
        }
        r.play_now = false;
        let policy = if n == 0 {
            r.settings.first_resign
        } else {
            r.settings.second_resign
        };
        let p = &mut r.sides[n];
        p.seconds += elapsed.as_secs_f64();
        p.visits += result.visits as f64;
        if best.winrate < policy.winrate && moves > policy.min_move {
            p.resign_count += 1;
        } else {
            p.resign_count = 0;
        }
        let resign_now =
            p.resign_count >= policy.consecutive_moves && best.winrate < policy.winrate && moves > policy.min_move;
        let random = r.values.random_moves_enabled && moves <= r.values.random_moves;
        let diff = r.values.random_winrate_diff;
        let check = r.settings.check_random_visits;
        let percent = r.settings.random_visits_percent;
        self.match_record_analysis(result, elapsed.as_secs_f64());
        if resign_now {
            self.match_end(MatchEnd::Resigned, Some(side.opposite()));
            return;
        }
        // Leelaz.randomBestmove: the first candidate is unconditional; others compare with
        // the maximum visits across all candidates, not necessarily the first candidate's visits.
        let max_visits = if random && check {
            result
                .candidates
                .iter()
                .map(|candidate| candidate.visits)
                .max()
                .unwrap_or(0)
        } else {
            0
        };
        let eligible = |candidate: &&CandidateView| {
            std::ptr::eq(*candidate, best)
                || (best.winrate - candidate.winrate <= diff
                    && (!check || candidate.visits as f64 >= max_visits as f64 * percent / 100.0))
        };
        let chosen = if random {
            let count = result.candidates.iter().filter(eligible).count();
            if count == 0 {
                best
            } else {
                result
                    .candidates
                    .iter()
                    .filter(eligible)
                    .nth(self.match_random(count))
                    .unwrap()
            }
        } else {
            best
        };
        self.match_commit(chosen.point, side, None);
    }
    fn match_end(&mut self, end: MatchEnd, winner: Option<Stone>) {
        let Some(r) = self.matches.run.as_mut() else { return };
        if r.phase == MatchPhase::BetweenGames {
            if end == MatchEnd::UserStopped {
                self.matches.run = None;
            }
            return;
        }
        if r.phase == MatchPhase::Ending {
            return;
        }
        let started = r.phase != MatchPhase::Starting;
        r.phase = MatchPhase::Ending;
        r.setup_deadline = Some(Instant::now() + MATCH_RETIRE);
        r.turn = None;
        r.komi_pending = None;
        r.komi_in_flight = None;
        r.komi_awaiting.clear();
        if matches!(end, MatchEnd::UserStopped | MatchEnd::Retired) {
            r.stop_batch = true;
        }
        let result = winner.map_or_else(String::new, |side| {
            format!("{}+R", if side == Stone::Black { "B" } else { "W" })
        });
        if started && !matches!(end, MatchEnd::UserStopped | MatchEnd::Retired) {
            r.completed += 1;
            let score = &mut r.score;
            score.total_seconds += r.sides.iter().map(|p| p.seconds).sum::<f64>();
            for (n, p) in r.sides.iter().enumerate() {
                r.total_seconds[n] += p.seconds;
            }
            score.first_visits += r.sides[0].visits;
            score.second_visits += r.sides[1].visits;
            record_outcome(score, end, winner, r.first_is_black);
            if let Some(opening) = r.opening.filter(|_| end == MatchEnd::Resigned) {
                let at = r
                    .opening_scores
                    .iter()
                    .position(|(i, _)| *i == opening as u32)
                    .unwrap_or_else(|| {
                        r.opening_scores.push((opening as u32, MatchScoreView::default()));
                        r.opening_scores.len() - 1
                    });
                if end == MatchEnd::Resigned {
                    record_outcome(&mut r.opening_scores[at].1, end, winner, r.first_is_black);
                }
            }
        }
        r.outcome = Some(MatchOutcomeView {
            game: r.game,
            ordinal: r.ordinal,
            games_dir: self
                .storage
                .save_dir
                .parent()
                .unwrap_or(Path::new("."))
                .join("EngineGames")
                .to_string_lossy()
                .into_owned(),
            batch: r.values.batch_enabled,
            end,
            winner,
            result: result.clone(),
            batch_finished: r.stop_batch || r.ordinal >= r.limit,
            score: r.score.clone(),
            saved_paths: Vec::new(),
            unrestored: Vec::new(),
        });
        for p in &r.sides {
            self.engines.cancel_genmove(p.reservation);
        }
        let save = started
            && !matches!(end, MatchEnd::UserStopped | MatchEnd::Retired)
            && (r.values.auto_save || r.values.batch_enabled);
        let _ = self.edit_document(DocumentEdit::SetResult { result });
        let _ = self.edit_document(DocumentEdit::SetAsMain { node: self.cursor });
        if save {
            self.match_record_totals();
            self.match_save(end, winner);
        }
        self.paused = true;
        self.sync_engine();
        self.match_release();
    }
    fn match_release(&mut self) {
        let Some(r) = self.matches.run.as_ref().filter(|r| r.phase == MatchPhase::Ending) else {
            return;
        };
        let seats: Vec<_> = r
            .sides
            .iter()
            .filter(|p| !p.released && p.awaiting.is_empty())
            .map(|p| {
                (
                    p.reservation,
                    p.restore.clone(),
                    p.original_rules
                        .clone()
                        .filter(|_| p.restore.iter().any(|command| command.starts_with("kata-set-rules "))),
                )
            })
            .collect();
        for (seat, restore, original) in seats {
            self.engines.release_game_verified(seat, &restore, original);
        }
        self.match_retirement();
    }
    fn match_retirement(&mut self) {
        let Some(r) = self.matches.run.as_mut() else { return };
        if r.phase == MatchPhase::BetweenGames && (r.stop_batch || r.ordinal >= r.limit) {
            self.matches.run = None;
            return;
        }
        if r.phase != MatchPhase::Ending || !r.sides.iter().all(|p| p.released) || r.image_pending {
            return;
        }
        let mut outcome = r.outcome.take().unwrap();
        outcome.batch_finished = r.stop_batch || r.ordinal >= r.limit;
        self.matches.outcome = Some(outcome);
        if r.stop_batch || r.ordinal >= r.limit {
            self.matches.run = None;
        } else {
            r.phase = MatchPhase::BetweenGames;
            r.next_game = Some(Instant::now() + Duration::from_millis(1));
        }
    }
    pub(super) fn match_deadline(&self) -> Option<Instant> {
        self.matches
            .run
            .as_ref()
            .and_then(|run| run.next_game.into_iter().chain(run.setup_deadline).min())
    }
    pub(super) fn match_tick(&mut self, now: Instant) -> bool {
        let expired = self
            .matches
            .run
            .as_ref()
            .is_some_and(|run| run.setup_deadline.is_some_and(|deadline| now >= deadline));
        if expired {
            let starting = self
                .matches
                .run
                .as_ref()
                .is_some_and(|run| run.phase == MatchPhase::Starting);
            if starting {
                self.matches.notices.push(MatchNotice {
                    message_key: "EngineManager.engineGameStartFailed".into(),
                    arguments: Vec::new(),
                });
                self.match_end(MatchEnd::Retired, None);
            }
            if let Some(run) = self.matches.run.as_mut() {
                run.setup_deadline = None;
                let seats = run.sides.each_ref().map(|participant| participant.reservation);
                for seat in seats {
                    self.engines.abandon_game(seat);
                }
            }
            self.drain_engine_events();
            return true;
        }
        let Some(r) = self
            .matches
            .run
            .as_mut()
            .filter(|r| r.phase == MatchPhase::BetweenGames && r.next_game.is_some_and(|d| now >= d))
        else {
            return false;
        };
        r.ordinal += 1;
        if r.values.exchange_colors {
            r.first_is_black = !r.first_is_black;
        }
        self.matches.next += 1;
        r.game = self.matches.next;
        r.phase = MatchPhase::Starting;
        r.next_game = None;
        r.setup_deadline = Some(now + MATCH_STARTUP);
        for p in &mut r.sides {
            let Ok(seat) = self.engines.reserve_game_instance(p.instance) else {
                self.match_end(MatchEnd::Retired, None);
                return true;
            };
            p.reservation = seat;
            p.setup = false;
            p.awaiting.clear();
            p.captures.clear();
            p.restore.clear();
            p.released = false;
            p.resign_count = 0;
            p.rules_verified = false;
            p.original_rules = None;
            p.observed_rules = None;
            p.seconds = 0.0;
            p.visits = 0.0;
        }
        r.previous = [None, None];
        self.match_prepare_board();
        self.match_setup();
        true
    }
    pub(super) fn match_revise_komi(&mut self, value: f64) -> bool {
        let Some(r) = self
            .matches
            .run
            .as_mut()
            .filter(|r| matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused))
        else {
            return false;
        };
        r.komi_pending = Some(value);
        self.sync_engine();
        self.match_apply_pending_komi();
        true
    }
    fn match_apply_pending_komi(&mut self) {
        let Some(r) = self.matches.run.as_mut().filter(|r| {
            r.turn.is_none()
                && r.komi_in_flight.is_none()
                && matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused)
        }) else {
            return;
        };
        let Some(komi) = r.komi_pending.take() else { return };
        r.komi_in_flight = Some(komi);
        for p in &r.sides {
            if let Some(command) = self.engines.game_send(p.reservation, &format!("komi {komi}")) {
                r.komi_awaiting.push((p.reservation, command));
            }
        }
    }
    pub(super) fn match_view(&self) -> Option<MatchView> {
        let r = self.matches.run.as_ref()?;
        let thinking = matches!(r.phase, MatchPhase::Playing | MatchPhase::Paused).then(|| self.match_side());
        let target = crate::engine::rules::KataRules::parse(&r.settings.rules);
        let rules_target_choice = target.and_then(|target| {
            [
                "chinese",
                "stone-scoring",
                "japanese",
                "aga",
                "new-zealand",
                "tromp-taylor",
                "goe",
            ]
            .iter()
            .find(|name| crate::engine::rules::KataRules::preset(name).is_some_and(|p| p == target))
            .map(|s| s.to_string())
        });
        let rules_unverified = r
            .sides
            .iter()
            .enumerate()
            .filter(|(_, p)| !p.rules_verified)
            .map(|(index, p)| {
                let identity = if index == 0 {
                    &r.settings.first
                } else {
                    &r.settings.second
                };
                MatchRulesUnverified {
                    name: p.name.clone(),
                    commands: identity.commands.clone(),
                    side: Some(if (index == 0) == r.first_is_black {
                        Stone::Black
                    } else {
                        Stone::White
                    }),
                    reason_key: "MatchRules.unconfirmed".into(),
                }
            })
            .collect();
        Some(MatchView {
            id: r.id,
            game: r.game,
            phase: r.phase,
            kind: r.values.kind,
            batch: r.values.batch_enabled,
            ordinal: r.ordinal,
            batch_limit: r.limit,
            completed_games: r.completed,
            first_engine: r.sides[0].index,
            second_engine: r.sides[1].index,
            first_name: r.sides[0].name.clone(),
            second_name: r.sides[1].name.clone(),
            first_is_black: r.first_is_black,
            score: r.score.clone(),
            thinking_side: thinking,
            thinking_engine: thinking.map(|side| r.sides[side_index(r.first_is_black, side)].index),
            manual_moves: r.manual,
            can_play_now: r.phase == MatchPhase::Playing && r.values.kind == GameKind::Analysis,
            can_toggle_manual: thinking.is_some() && r.values.kind == GameKind::Analysis,
            can_resign: thinking.is_some(),
            rules: r.settings.rules.clone(),
            rules_consent_required: r.phase == MatchPhase::Starting
                && r.sides.iter().all(|p| p.setup && p.awaiting.is_empty())
                && !r.rules_consent
                && r.sides.iter().any(|p| !p.rules_verified),
            rules_unverified,
            rules_target_choice,
            opening_index: r.opening.map(|i| i as u32),
            elapsed_seconds: r.started.elapsed().as_secs_f64(),
        })
    }
    pub(super) fn take_match_outcome(
        &mut self,
    ) -> (
        Option<MatchOutcomeView>,
        Vec<MatchNotice>,
        Option<MatchWinrateImageRequest>,
    ) {
        (
            self.matches.outcome.take(),
            std::mem::take(&mut self.matches.notices),
            self.matches.image.take(),
        )
    }
    pub(super) fn match_outcome_copy(
        &self,
    ) -> (
        Option<MatchOutcomeView>,
        Vec<MatchNotice>,
        Option<MatchWinrateImageRequest>,
    ) {
        (
            self.matches.outcome.clone(),
            self.matches.notices.clone(),
            self.matches.image.clone(),
        )
    }
    pub fn match_winrate_path(&self, game: u64) -> Option<PathBuf> {
        self.matches
            .image_write
            .as_ref()
            .filter(|r| r.game == game)
            .map(|r| PathBuf::from(&r.path))
    }
    pub(super) fn take_match_saves(&mut self) -> Vec<SaveJob> {
        std::mem::take(&mut self.matches.jobs)
    }
    pub(super) fn match_save_finished(&mut self, ticket: u64, result: &Result<(), String>) -> bool {
        let Some(at) = self.matches.saves.iter().position(|t| *t == ticket) else {
            return false;
        };
        self.matches.saves.remove(at);
        if let Err(error) = result {
            self.log(true, format!("saving engine match failed: {error}"));
        }
        true
    }
    fn match_save(&mut self, end: MatchEnd, winner: Option<Stone>) {
        let r = self.matches.run.as_ref().unwrap();
        let black = side_index(r.first_is_black, Stone::Black);
        let names = (&r.sides[black].name, &r.sides[1 - black].name);
        let prefix = if r.values.batch_enabled {
            format!(
                "{}_{}",
                r.ordinal
                    + if r.values.start_number_enabled {
                        r.values.start_number.saturating_sub(1)
                    } else {
                        0
                    },
                r.opening.map_or_else(String::new, |i| format!(
                    "{}{}_",
                    super::exporting::java_text(self.active_language, "EngineGameInfo.openingSGFindex"),
                    i
                ))
            )
        } else {
            String::new()
        };
        let text = |key| self.match_text(key);
        let result = match end {
            MatchEnd::DoublePass => text("EngineManager.doublePassFileName"),
            MatchEnd::MaxMoves => text("EngineManager.outOfMoveFileName"),
            _ => {
                let side = winner.unwrap();
                format!(
                    "_{}({}){}",
                    text(if side == Stone::Black {
                        "Leelaz.black"
                    } else {
                        "Leelaz.white"
                    }),
                    if side == Stone::Black { names.0 } else { names.1 },
                    text("Leelaz.win")
                )
            }
        };
        let name = format!(
            "{prefix}{}({})_vs_{}({}){result}_{}",
            text("Leelaz.black"),
            names.0,
            text("Leelaz.white"),
            names.1,
            chrono::Local::now().format("%Y%m%d%H%M%S")
        );
        let path = r.folder.join(format!("{}.sgf", safe(&name)));
        let image = r.values.save_winrate;
        let batch = r.values.batch_enabled;
        let game = r.game;
        let summary_path = r.folder.join(format!("{}{}.txt", text("Leelaz.result"), r.timestamp));
        let summary = self.match_summary();
        let clean = !self.doc.unsaved();
        let mut job = self.begin_save(&path.to_string_lossy(), "LizzieYzy Tauri");
        if clean {
            self.doc.saved_revision = Some(self.doc.revision);
        }
        job.create_parent = true;
        self.matches.saves.push(job.ticket);
        self.matches.jobs.push(job);
        let r = self.matches.run.as_mut().unwrap();
        if let Some(outcome) = &mut r.outcome {
            outcome.saved_paths.push(path.to_string_lossy().into_owned());
        }
        if image {
            let request = MatchWinrateImageRequest {
                game,
                path: path.with_extension("png").to_string_lossy().into_owned(),
            };
            r.image_pending = true;
            self.matches.image = Some(request.clone());
            self.matches.image_write = Some(request);
        }
        if batch {
            let mut job = self.begin_save(&summary_path.to_string_lossy(), "LizzieYzy Tauri");
            job.text = summary;
            job.create_parent = true;
            self.matches.saves.push(job.ticket);
            self.matches.jobs.push(job);
        }
    }
}
