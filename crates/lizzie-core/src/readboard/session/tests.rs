//! Session behaviour through its public surface: helper lines in, record changes and
//! [`SessionEvent`]s out, against an in-memory [`SyncBoard`]. Cases follow Java's
//! `ReadBoardSyncDecisionTest`, `ReadBoardPendingLocalMoveSyncTest`,
//! `ReadBoardFoxMoveNumberParsingTest`, `ReadBoardSnapshotMetadataCopyTest` and
//! `ReadBoardResumeLifecycleTest`.

use std::time::{Duration, Instant};

use super::*;
use crate::record::{BoardSize, GameRecord, NodeId, NodeKind, Point, Property, Stone, ROOT};

const B: Stone = Stone::Black;
const W: Stone = Stone::White;

fn p(x: u16, y: u16) -> Point {
    Point { x, y }
}

/// A record and a view, changed the way the workbench's implementation changes the document.
struct Board {
    rec: GameRecord,
    cursor: NodeId,
    rebuilt: Vec<RebuiltSnapshot>,
}

impl Board {
    fn new(n: u16) -> Board {
        Board { rec: GameRecord::empty(BoardSize { width: n, height: n }, 7.5), cursor: ROOT, rebuilt: Vec::new() }
    }

    /// A local click at the main end.
    fn play(&mut self, color: Stone, x: u16, y: u16) -> NodeId {
        self.place_for_sync(color, p(x, y)).expect("legal test move")
    }

    fn main_line(&self) -> Vec<(Stone, Point)> {
        let mut moves = Vec::new();
        let mut node = ROOT;
        while let Some(child) = next_excluding_end_placeholder(&self.rec, node) {
            if let NodeKind::Move { color, point } = self.rec.node(child).kind {
                moves.push((color, point));
            }
            node = child;
        }
        moves
    }

    fn stone(&self, x: u16, y: u16) -> Option<Stone> {
        self.rec.position(self.main_end()).at(p(x, y))
    }
}

impl SyncBoard for Board {
    fn record(&self) -> &GameRecord {
        &self.rec
    }

    fn cursor(&self) -> NodeId {
        self.cursor
    }

    fn place_for_sync(&mut self, color: Stone, point: Point) -> Option<NodeId> {
        let end = self.main_end();
        let mut pos = self.rec.position(end);
        if !self.rec.size.contains(point) || pos.at(point).is_some() || pos.play(color, point) > 0 {
            return None;
        }
        let id = self.rec.push_child(end, NodeKind::Move { color, point });
        let children = &mut self.rec.node_mut(end).children;
        children.retain(|&c| c != id);
        children.insert(0, id);
        self.cursor = id;
        Some(id)
    }

    fn rebuild(&mut self, snapshot: RebuiltSnapshot) -> NodeId {
        let size = self.rec.size;
        let mut rec = GameRecord::empty(size, self.rec.info.komi);
        rec.info = self.rec.info.clone();
        rec.resume_after(&self.rec);
        let root = rec.node_mut(ROOT);
        for (i, stone) in snapshot.stones.iter().enumerate() {
            let point = p((i % size.width as usize) as u16, (i / size.width as usize) as u16);
            match stone {
                Some(Stone::Black) => root.setup.add_black.push(point),
                Some(Stone::White) => root.setup.add_white.push(point),
                None => {}
            }
        }
        root.to_play = snapshot.to_play;
        root.explicit_to_play = snapshot.explicit_to_play;
        root.move_number = snapshot.move_number;
        if let Some(metadata) = &snapshot.metadata {
            root.comment.clone_from(&metadata.comment);
            root.properties.clone_from(&metadata.properties);
        }
        rec.restamp(ROOT);
        self.rec = rec;
        self.cursor = ROOT;
        self.rebuilt.push(snapshot);
        ROOT
    }

    fn move_to(&mut self, node: NodeId) {
        self.cursor = node;
    }

    fn discard_failed_move(&mut self, node: NodeId) -> Option<NodeId> {
        if self.main_end() != node {
            return None;
        }
        let parent = self.rec.parent(node)?;
        self.rec.node_mut(parent).children.retain(|&c| c != node);
        self.rec.node_mut(node).live = false;
        self.cursor = parent;
        Some(parent)
    }

    fn clear_board(&mut self) {
        let mut rec = GameRecord::empty(self.rec.size, self.rec.info.komi);
        rec.resume_after(&self.rec);
        self.rec = rec;
        self.cursor = ROOT;
    }

    fn reopen(&mut self, width: u8, height: u8) {
        let mut rec = GameRecord::empty(BoardSize { width: width.into(), height: height.into() }, self.rec.info.komi);
        rec.resume_after(&self.rec);
        self.rec = rec;
        self.cursor = ROOT;
    }

    fn change_next_turn(&mut self) {
        let node = self.rec.node_mut(self.cursor);
        node.to_play = node.to_play.opposite();
        self.rec.restamp(self.cursor);
    }
}

struct Harness {
    s: ReadBoardSession,
    b: Board,
    ctx: HostContext,
    now: Instant,
}

impl Harness {
    fn new() -> Harness {
        Harness {
            s: ReadBoardSession::new(SessionSettings::default()),
            b: Board::new(5),
            ctx: HostContext {
                engine_available: true,
                can_resume_analysis: true,
                can_resume_auto_play_analysis: true,
                ..HostContext::default()
            },
            now: Instant::now(),
        }
    }

    fn line(&mut self, line: &str) -> Vec<SessionEvent> {
        self.s.line(&mut self.b, line, &self.ctx, self.now)
    }

    fn lines(&mut self, lines: &[&str]) -> Vec<SessionEvent> {
        lines.iter().flat_map(|l| self.line(l)).collect()
    }

    /// `re=` rows written as digit strings (`"00300"`), then `end`.
    fn frame(&mut self, rows: &[&str]) -> Vec<SessionEvent> {
        self.frame_with(&[], rows)
    }

    /// Remote-context lines, the rows and `end`.
    fn frame_with(&mut self, context: &[String], rows: &[&str]) -> Vec<SessionEvent> {
        let mut events = Vec::new();
        for line in context {
            events.extend(self.line(line));
        }
        for row in rows {
            let cells: Vec<String> = row.chars().map(String::from).collect();
            events.extend(self.line(&format!("re={}", cells.join(","))));
        }
        events.extend(self.line("end"));
        events
    }

    fn advance(&mut self, ms: u64) -> Vec<SessionEvent> {
        self.now += Duration::from_millis(ms);
        self.s.tick(&mut self.b, &self.ctx, self.now)
    }

    fn local_move(&mut self, color: Stone, x: u16, y: u16) -> (NodeId, Vec<SessionEvent>) {
        let node = self.b.play(color, x, y);
        let events = self.s.local_move(&mut self.b, p(x, y), &self.ctx, self.now);
        (node, events)
    }

    fn gate(&mut self, color: Stone, x: u16, y: u16) -> PlaceGate {
        self.s.place_gate(&self.b, p(x, y), color, self.now)
    }

    fn both_sync() -> Harness {
        let mut h = Harness::new();
        h.lines(&["sync", "bothSync"]);
        h
    }
}

fn fox(number: i32, title: i32) -> Vec<String> {
    vec![
        "syncPlatform fox".to_owned(),
        "roomToken 43581".to_owned(),
        format!("liveTitleMove {title}"),
        format!("foxMoveNumber {number}"),
    ]
}

fn sends(events: &[SessionEvent]) -> Vec<&str> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Send(line) => Some(line.as_str()),
            _ => None,
        })
        .collect()
}

fn decisions(events: &[SessionEvent]) -> Vec<(RecoveryOutcome, &'static str)> {
    events
        .iter()
        .filter_map(|e| match e {
            SessionEvent::Decision(d) => Some((d.outcome, d.reason)),
            _ => None,
        })
        .collect()
}

const EMPTY: [&str; 5] = ["00000", "00000", "00000", "00000", "00000"];

// ------------------------------------------------------------------- protocol lines

#[test]
fn only_the_first_ready_starts_the_handshake() {
    let mut h = Harness::new();
    let events = h.line("ready");
    assert_eq!(
        sends(&events),
        ["version", "readboardUpdateSupported", "readboardUpdatePackageV2Supported", "analysisState paused"]
    );
    assert!(events.contains(&SessionEvent::LoggingReady));
    assert!(h.s.loaded());
    assert!(h.line("ready").is_empty());
}

#[test]
fn helper_versions_below_the_minimum_are_reported() {
    let mut h = Harness::new();
    let old = h.line("version 220429");
    assert!(old.contains(&SessionEvent::VersionTooOld));
    assert!(old.contains(&SessionEvent::ConsoleLine("Board synchronization tool version 220429".to_owned())));
    assert!(!h.line("version 220430").contains(&SessionEvent::VersionTooOld));
    assert!(!h.line("version abc").contains(&SessionEvent::VersionTooOld));
}

#[test]
fn inboard_accepts_plain_and_scaled_geometry() {
    let mut h = Harness::new();
    let plain = h.line("inboard 10 20 300 310 1");
    assert_eq!(
        plain,
        [SessionEvent::FloatBoard(FloatBoardEvent::Show(FloatBoardGeometry {
            x: 10,
            y: 20,
            width: 300,
            height: 310,
            board_type: 1,
            scale: None,
        }))]
    );
    let scaled = h.line("inboard 10 20 300 311 99_1.5_2");
    assert_eq!(
        scaled,
        [SessionEvent::FloatBoard(FloatBoardEvent::Show(FloatBoardGeometry {
            x: 15,
            y: 30,
            width: 450,
            height: 467,
            board_type: 2,
            scale: Some(1.5),
        }))]
    );
    assert!(h.s.float_board().shown);
    assert_eq!(h.line("notinboard"), [SessionEvent::FloatBoard(FloatBoardEvent::HideAndForget)]);
    assert!(!h.s.float_board().shown);
}

#[test]
fn a_malformed_row_invalidates_the_whole_frame() {
    let mut h = Harness::new();
    h.lines(&["re=0,0,3", "re=0,0,0,0,0"]);
    h.frame(&["00000", "00000", "00300", "00000"]);
    assert!(h.b.main_line().is_empty());
    h.lines(&["re=0,0,0,0,0", "re=0,0,0,0,0", "re=0,0,3,0,x", "re=0,0,0,0,0", "re=0,0,0,0,0", "end"]);
    assert!(h.b.main_line().is_empty());
    // The next frame is read again.
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
}

#[test]
fn start_with_another_size_reopens_the_board() {
    let mut h = Harness::new();
    let events = h.line("start 9 9");
    assert_eq!(h.b.rec.size, BoardSize { width: 9, height: 9 });
    assert!(events.contains(&SessionEvent::ResumeCancelled));
    h.line("start x 9");
    assert_eq!(h.b.rec.size, BoardSize { width: 9, height: 9 });
}

#[test]
fn pass_changes_the_side_to_play_and_trusts_the_turn() {
    let mut h = Harness::new();
    let events = h.line("pass");
    assert!(events.contains(&SessionEvent::PassLine));
    assert_eq!(h.b.rec.node(ROOT).to_play, W);
    assert!(h.s.turn_trusted());
}

#[test]
fn update_logging_and_yike_lines_are_routed_and_yike_frames_are_not_applied() {
    let mut h = Harness::new();
    let update = "readboardUpdateReady\tv1\tC:\\u.zip";
    assert_eq!(h.line(update), [SessionEvent::UpdateReady(update.to_owned())]);
    assert_eq!(h.line("readboardLoggingV1 x"), [SessionEvent::LoggingLine("readboardLoggingV1 x".to_owned())]);
    assert_eq!(h.line("yikeSyncStart"), [SessionEvent::YikeSyncStart { from_platform_line: false }]);
    assert_eq!(h.line("yikeSyncStop"), [SessionEvent::YikeSyncStop]);
    let events = h.frame_with(&["syncPlatform yike".to_owned()], &["00000", "00000", "00300", "00000", "00000"]);
    assert!(events.contains(&SessionEvent::YikeSyncStart { from_platform_line: true }));
    assert!(h.b.main_line().is_empty());
}

#[test]
fn focus_is_returned_with_loss_when_enabled() {
    let h = Harness::new();
    assert_eq!(sends(&h.s.return_focus()), ["loss"]);
    let quiet = ReadBoardSession::new(SessionSettings { read_board_get_focus: false, ..SessionSettings::default() });
    assert!(quiet.return_focus().is_empty());
}

// ------------------------------------------------------------------------ frames

#[test]
fn marked_new_stones_are_played_on_the_main_line() {
    let mut h = Harness::new();
    assert!(h.line("sync").contains(&SessionEvent::StartAnalysis));
    let events = h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
    assert!(decisions(&events).is_empty());
    assert!(events.iter().any(|e| matches!(e, SessionEvent::Synced { reason: "sync", .. })));
    h.frame(&["00000", "00000", "00100", "00040", "00000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2)), (W, p(3, 3))]);
    assert_eq!(h.b.cursor, h.b.main_end());
    assert!(h.s.turn_trusted());
    assert_eq!(h.s.resolved_node(), Some(h.b.main_end()));

    // The same position again changes nothing.
    let again = h.frame(&["00000", "00000", "00100", "00020", "00000"]);
    assert_eq!(h.b.main_line().len(), 2);
    assert!(decisions(&again).is_empty());
}

#[test]
fn a_capture_by_one_marked_move_is_replayed() {
    let mut h = Harness::new();
    h.b.play(W, 0, 0);
    h.b.play(B, 1, 0);
    h.b.play(W, 4, 4);
    let events = h.frame(&["01000", "30000", "00000", "00000", "00002"]);
    assert_eq!(decisions(&events), [(RecoveryOutcome::SingleMoveRecovery, "single_move_recovery")]);
    assert_eq!(h.b.main_line(), [(W, p(0, 0)), (B, p(1, 0)), (W, p(4, 4)), (B, p(0, 1))]);
    assert_eq!(h.b.stone(0, 0), None);
    assert_eq!(h.b.cursor, h.b.main_end());
    assert!(h.b.rebuilt.is_empty());
}

#[test]
fn a_conflicting_frame_is_held_once_then_rebuilt() {
    let mut h = Harness::new();
    h.b.play(B, 2, 2);
    // The same point now shows the other colour.
    let changed = ["00000", "00000", "00200", "00000", "00000"];
    let first = h.frame(&changed);
    assert_eq!(decisions(&first), [(RecoveryOutcome::Hold, "conflict_hold")]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);

    let second = h.frame(&changed);
    assert_eq!(decisions(&second), [(RecoveryOutcome::ForceRebuild, "fallback_force_rebuild")]);
    assert!(h.b.main_line().is_empty());
    assert_eq!(h.b.rec.node(ROOT).setup.add_white, [p(2, 2)]);
    assert_eq!(h.b.cursor, ROOT);
    assert_eq!(h.s.resolved_node(), Some(ROOT));
    assert!(second.contains(&SessionEvent::Synced { node: ROOT, reason: "rebuild" }));
}

#[test]
fn force_rebuild_skips_the_hold() {
    let mut h = Harness::new();
    h.b.play(B, 2, 2);
    h.line("forceRebuild");
    let events = h.frame(&["00000", "00000", "00200", "00000", "00000"]);
    assert_eq!(decisions(&events), [(RecoveryOutcome::ForceRebuild, "remote_force_rebuild_requested")]);
    assert_eq!(h.b.rebuilt.len(), 1);
}

#[test]
fn a_fox_colour_change_with_another_move_number_rebuilds_at_once() {
    let mut h = Harness::new();
    h.frame_with(&fox(1, 1), &["00000", "00000", "00300", "00000", "00000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
    let events = h.frame_with(&fox(2, 2), &["00000", "00000", "00200", "00000", "00000"]);
    assert_eq!(decisions(&events), [(RecoveryOutcome::ForceRebuild, "fallback_force_rebuild")]);
    let snapshot = h.b.rebuilt.last().expect("rebuilt");
    assert_eq!(snapshot.move_number, 2);
    assert_eq!(h.b.rec.node(ROOT).setup.add_white, [p(2, 2)]);
}

#[test]
fn the_first_fox_frame_that_matches_nothing_rebuilds() {
    let mut h = Harness::new();
    h.b.play(B, 0, 0);
    assert!(h.s.awaiting_first_sync_frame());
    let events = h.frame_with(
        &["syncPlatform fox".to_owned(), "foxMoveNumber 5".to_owned()],
        &["00000", "00000", "00100", "00020", "00000"],
    );
    assert_eq!(decisions(&events), [(RecoveryOutcome::ForceRebuild, "first_sync_force_rebuild")]);
    assert_eq!(h.b.rec.node(ROOT).move_number, 5);
    assert!(!h.s.awaiting_first_sync_frame());
}

#[test]
fn a_fox_move_number_change_alone_rebuilds_the_snapshot() {
    let mut h = Harness::new();
    let fox2 = ["syncPlatform fox".to_owned(), "foxMoveNumber 2".to_owned()];
    h.frame_with(&fox2, &EMPTY);
    assert_eq!(h.b.rebuilt.len(), 1);
    assert_eq!(h.b.rec.node(ROOT).move_number, 2);
    assert_eq!(h.b.rec.node(ROOT).to_play, B);
    // The rebuilt snapshot now agrees with the remote metadata.
    h.frame_with(&fox2, &EMPTY);
    assert_eq!(h.b.rebuilt.len(), 1);
}

#[test]
fn a_lagging_live_title_keeps_the_accepted_move() {
    let mut h = Harness::new();
    h.frame_with(&fox(1, 1), &["00000", "00000", "00300", "00000", "00000"]);
    h.frame_with(&fox(2, 2), &["00000", "00000", "00100", "00040", "00000"]);
    assert_eq!(h.b.main_line().len(), 2);
    // The title still names the previous move.
    let events = h.frame_with(&fox(1, 1), &["00000", "00000", "00100", "00040", "00000"]);
    assert_eq!(
        decisions(&events),
        [(RecoveryOutcome::NoChange, "snapshot_matches_accepted_move_with_lagging_title")]
    );
    assert_eq!(h.b.main_line(), [(B, p(2, 2)), (W, p(3, 3))]);
    assert!(h.b.rebuilt.is_empty());
}

#[test]
fn a_rebuild_copies_the_anchor_metadata() {
    let mut h = Harness::new();
    {
        let root = h.b.rec.node_mut(ROOT);
        root.comment = "intro".to_owned();
        root.properties.push(Property { id: "GN".to_owned(), values: vec!["game".to_owned()] });
        root.explicit_to_play = Some(W);
        root.to_play = W;
    }
    let events = h.frame(&["10000", "01000", "00000", "00000", "00000"]);
    assert_eq!(decisions(&events), [(RecoveryOutcome::ForceRebuild, "fallback_force_rebuild")]);
    let snapshot = h.b.rebuilt.last().expect("rebuilt");
    assert_eq!(snapshot.explicit_to_play, Some(W));
    assert_eq!(snapshot.to_play, W);
    let root = h.b.rec.node(ROOT);
    assert_eq!(root.comment, "intro");
    assert_eq!(root.properties[0].id, "GN");
    assert_eq!(root.setup.add_black, [p(0, 0), p(1, 1)]);
}

#[test]
fn a_rebuild_is_abandoned_when_the_anchor_move_number_is_not_a_number() {
    let mut h = Harness::new();
    h.b.rec.node_mut(ROOT).properties.push(Property { id: "MN".to_owned(), values: vec!["x".to_owned()] });
    h.frame(&["10000", "01000", "00000", "00000", "00000"]);
    assert!(h.b.rebuilt.is_empty());
    assert!(h.b.rec.node(ROOT).setup.is_empty());
}

// ------------------------------------------------------------------ view and resume

#[test]
fn the_first_frame_outside_analysis_shows_the_previous_move_then_the_newest() {
    let mut h = Harness::new();
    h.ctx.playing_against_engine = true;
    h.line("sync");
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    let end = h.b.main_end();
    assert_ne!(end, ROOT);
    assert_eq!(h.b.cursor, ROOT);
    assert_eq!(h.s.next_deadline(), Some(h.now + Duration::from_millis(500)));
    h.advance(499);
    assert_eq!(h.b.cursor, ROOT);
    h.advance(1);
    assert_eq!(h.b.cursor, end);
}

#[test]
fn analysis_resumes_200ms_after_a_sync_only_with_playpon_on() {
    let mut h = Harness::new();
    h.lines(&["sync", "playpon on"]);
    assert!(h.s.read_board_ponder());
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    let node = h.b.main_end();
    assert!(!h.advance(199).iter().any(|e| matches!(e, SessionEvent::ResumeAnalysis { .. })));
    assert_eq!(h.advance(1), [SessionEvent::ResumeAnalysis { node }]);

    let mut off = Harness::new();
    off.lines(&["sync", "playpon off"]);
    off.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert!(off.advance(200).is_empty());
}

#[test]
fn user_navigation_cancels_the_scheduled_resume() {
    let mut h = Harness::new();
    h.lines(&["sync", "playpon on"]);
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert_eq!(h.s.local_navigation(), [SessionEvent::ResumeCancelled]);
    assert!(h.advance(200).is_empty());
}

/// Java reads `applySyncViewState`'s `currentNode` after `placeForSync` moved the view to the new
/// move (`ReadBoard.java:1984-1987`), so even one-way sync with 总是保持棋盘一致 off ends on it.
#[test]
fn a_synced_move_is_shown_even_after_browsing_back() {
    let mut h = Harness::new();
    h.s.set_settings(SessionSettings { always_sync_board_stat: false, ..SessionSettings::default() });
    h.line("sync");
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    h.b.move_to(ROOT);
    h.s.local_navigation();
    h.frame(&["00000", "00000", "00100", "00040", "00000"]);
    assert_eq!(h.b.cursor, h.b.main_end());
}

#[test]
fn endsync_and_stopsync_end_syncing() {
    let mut h = Harness::new();
    h.ctx.analysis_game = true;
    h.line("sync");
    let end = h.line("endsync");
    assert!(!h.s.sync_board());
    assert!(end.contains(&SessionEvent::GmaCleared { reason: "endsync" }));
    assert!(end.contains(&SessionEvent::StopGame));
    assert!(!end.contains(&SessionEvent::StopAnalysis));

    h.line("sync");
    let stop = h.line("stopsync");
    assert!(!h.s.sync_board());
    assert!(stop.contains(&SessionEvent::GmaCleared { reason: "stopsync" }));
    assert!(stop.contains(&SessionEvent::StopAnalysis));
}

#[test]
fn noponder_and_resumeponder_report_the_analysis_state() {
    let mut h = Harness::new();
    h.ctx.pondering = true;
    h.ctx.playing_against_engine = true;
    let stop = h.line("noponder");
    assert!(stop.contains(&SessionEvent::StopPlayingAgainstEngine));
    assert!(stop.contains(&SessionEvent::StopAnalysis));
    assert_eq!(sends(&stop), ["analysisState paused"]);

    h.ctx.pondering = false;
    h.ctx.playing_against_engine = false;
    let resume = h.line("resumeponder");
    assert!(resume.contains(&SessionEvent::ToggleAnalysisManual));
    assert_eq!(sends(&resume), ["analysisState running"]);

    h.ctx.engine_available = false;
    let idle = h.line("resumeponder");
    assert!(!idle.contains(&SessionEvent::ToggleAnalysisManual));
    assert_eq!(sends(&idle), ["analysisState paused"]);
}

#[test]
fn shutdown_sends_quit_once_and_not_after_the_helper_ended() {
    let mut h = Harness::new();
    h.lines(&["sync", "bothSync"]);
    assert_eq!(sends(&h.s.shutdown(&mut h.b, &h.ctx, h.now)), ["quit"]);
    assert!(h.s.shut_down());
    assert!(!h.s.sync_board() && !h.s.both_sync());
    assert!(h.s.shutdown(&mut h.b, &h.ctx, h.now).is_empty());

    let mut ended = Harness::new();
    ended.ctx.pondering = true;
    let events = ended.s.input(&mut ended.b, &HelperInput::Ended { error: None }, &ended.ctx, ended.now);
    assert!(events.contains(&SessionEvent::ResumeAnalysis { node: ROOT }));
    assert!(sends(&events).is_empty());
    assert!(ended.s.shutdown(&mut ended.b, &ended.ctx, ended.now).is_empty());
}

// ----------------------------------------------------------------- local moves

#[test]
fn a_local_move_is_sent_and_confirmed_by_the_frame_that_shows_it() {
    let mut h = Harness::both_sync();
    let (node, events) = h.local_move(B, 2, 2);
    assert_eq!(sends(&events), ["place 2 2"]);
    assert_eq!(
        h.s.pending_local_move(),
        Some(PendingLocalMove { node: Some(node), point: Some(p(2, 2)), color: Some(B), click_completed: false })
    );
    h.line("placeComplete");
    assert!(h.s.pending_local_move().is_some_and(|m| m.click_completed));

    let frame = h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert!(frame.contains(&SessionEvent::LocalMoveConfirmed { node }));
    assert!(frame.contains(&SessionEvent::Synced { node, reason: "ack" }));
    assert_eq!(h.s.pending_local_move(), None);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
}

#[test]
fn local_moves_are_not_sent_without_both_way_sync() {
    let mut h = Harness::new();
    h.line("sync");
    let (_, events) = h.local_move(B, 2, 2);
    assert!(events.is_empty());
    assert_eq!(h.s.pending_local_move(), None);
}

#[test]
fn a_pending_move_holds_frames_without_it_and_times_out() {
    let mut h = Harness::both_sync();
    h.local_move(B, 2, 2);
    let held = h.frame(&EMPTY);
    assert_eq!(decisions(&held), [(RecoveryOutcome::Hold, "conflict_hold")]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
    assert_eq!(h.gate(W, 3, 3), PlaceGate::Suppressed);

    assert_eq!(h.s.next_deadline(), Some(h.now + PENDING_LOCAL_MOVE_ACK_TIMEOUT));
    let events = h.advance(3000);
    assert!(events.iter().any(|e| matches!(e, SessionEvent::LocalMoveFailed { node: Some(ROOT), .. })));
    assert!(h.b.main_line().is_empty());
    assert_eq!(h.b.cursor, ROOT);
    assert_eq!(h.s.pending_local_move(), None);
    assert!(h.s.failed_local_move_awaiting_observation());
}

#[test]
fn a_failed_place_rolls_back_and_holds_local_moves_for_the_grace() {
    let mut h = Harness::both_sync();
    h.local_move(B, 2, 2);
    let events = h.line("error place failed");
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::LocalMoveFailed { node: Some(ROOT), reason } if reason.contains("placement failed")
    )));
    assert!(events.contains(&SessionEvent::ConsoleLine("error place failed".to_owned())));
    assert!(h.b.main_line().is_empty());

    assert_eq!(h.gate(B, 2, 2), PlaceGate::Suppressed);
    assert_eq!(h.gate(B, 3, 3), PlaceGate::Suppressed);
    h.now += FAILED_LOCAL_MOVE_OBSERVATION_GRACE;
    assert_eq!(h.gate(B, 3, 3), PlaceGate::Proceed);
    assert!(!h.s.failed_local_move_awaiting_observation());
}

#[test]
fn a_frame_showing_the_failed_stone_ends_the_failure() {
    let mut h = Harness::both_sync();
    h.local_move(B, 2, 2);
    h.line("error place failed");
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    assert!(!h.s.failed_local_move_awaiting_observation());
    assert_eq!(h.b.main_line(), [(B, p(2, 2))]);
    assert_eq!(h.gate(W, 3, 3), PlaceGate::Proceed);
}

#[test]
fn a_remote_change_without_the_pending_stone_fails_it() {
    let mut h = Harness::both_sync();
    h.local_move(B, 2, 2);
    let events = h.frame(&["00000", "00000", "00000", "00000", "00004"]);
    assert!(events.iter().any(|e| matches!(
        e,
        SessionEvent::LocalMoveFailed { reason, .. } if reason == "pending local move remote changed without target"
    )));
    assert!(h.b.main_line().is_empty());
}

#[test]
fn a_late_place_complete_does_not_complete_the_next_move() {
    let mut h = Harness::both_sync();
    h.local_move(B, 2, 2);
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    h.local_move(W, 3, 3);
    // The click result of the confirmed move arrives late.
    h.line("placeComplete");
    assert!(h.s.pending_local_move().is_some_and(|m| !m.click_completed));
}

#[test]
fn a_click_at_the_main_end_during_one_way_sync_becomes_a_variation() {
    let mut h = Harness::new();
    h.line("sync");
    h.b.play(B, 2, 2);
    assert_eq!(h.gate(W, 3, 3), PlaceGate::Variation);
    h.b.move_to(ROOT);
    assert_eq!(h.gate(W, 3, 3), PlaceGate::Proceed);
    h.line("bothSync");
    h.b.move_to(h.b.main_end());
    assert_eq!(h.gate(W, 3, 3), PlaceGate::Proceed);
}

#[test]
fn a_float_board_that_is_not_on_screen_does_not_delay_the_place() {
    let mut h = Harness::both_sync();
    h.lines(&["inboard 1 2 3 4 1", "foreFoxWithInBoard"]);
    let (_, events) = h.local_move(B, 2, 2);
    assert_eq!(
        events,
        [
            SessionEvent::FloatBoard(FloatBoardEvent::HideForPlace { place_waits: false }),
            SessionEvent::Send("place 2 2".to_owned())
        ]
    );
    assert!(h.line("inboard 1 2 3 4 1").is_empty());
    assert_eq!(h.line("placeComplete"), [SessionEvent::FloatBoard(FloatBoardEvent::RestoreAfterPlace)]);
}

#[test]
fn a_shown_float_board_is_hidden_before_the_place_is_written_once() {
    let mut h = Harness::both_sync();
    h.lines(&["inboard 1 2 3 4 1", "foreFoxWithInBoard"]);
    h.ctx.float_board_shown = true;
    let (_, events) = h.local_move(B, 2, 2);
    assert_eq!(events, [SessionEvent::FloatBoard(FloatBoardEvent::HideForPlace { place_waits: true })]);
    assert!(h.s.float_board().hidden_for_place);
    // Repeated `inboard` lines while it waits do not bring the window back.
    assert!(h.line("inboard 1 2 3 4 1").is_empty());
    h.ctx.float_board_shown = false;
    let hidden = h.s.float_board_hidden(&mut h.b, &h.ctx, h.now);
    assert_eq!(sends(&hidden), ["place 2 2"]);
    assert!(h.s.float_board_hidden(&mut h.b, &h.ctx, h.now).is_empty(), "a second confirmation sends nothing");
    assert!(sends(&h.advance(1000)).is_empty());
    assert_eq!(h.line("placeComplete"), [SessionEvent::FloatBoard(FloatBoardEvent::RestoreAfterPlace)]);
}

#[test]
fn an_unconfirmed_hide_never_places_and_the_move_fails_at_its_ack_timeout() {
    let mut h = Harness::both_sync();
    h.lines(&["inboard 1 2 3 4 1", "foreFoxWithInBoard"]);
    h.ctx.float_board_shown = true;
    h.local_move(B, 2, 2);
    assert_eq!(h.s.next_deadline(), Some(h.now + PENDING_LOCAL_MOVE_ACK_TIMEOUT));
    assert!(sends(&h.advance(2999)).is_empty(), "no click while the window may still cover the target");
    let events = h.advance(1);
    assert!(sends(&events).is_empty());
    assert!(events.iter().any(|e| matches!(e, SessionEvent::LocalMoveFailed { .. })), "{events:?}");
    assert!(h.s.float_board_hidden(&mut h.b, &h.ctx, h.now).is_empty(), "a late confirmation sends nothing");
}

#[test]
fn a_superseded_move_is_never_placed_after_the_float_board_hid() {
    let mut h = Harness::both_sync();
    h.lines(&["inboard 1 2 3 4 1", "foreFoxWithInBoard"]);
    h.ctx.float_board_shown = true;
    h.local_move(B, 2, 2);
    h.s.history_overwritten();
    assert!(h.s.float_board_hidden(&mut h.b, &h.ctx, h.now).is_empty());
    assert!(sends(&h.advance(1000)).is_empty());
    assert_eq!(h.s.next_deadline(), None);
}

#[test]
fn in_the_float_board_edit_mode_a_synced_move_keeps_the_viewed_node() {
    let mut h = Harness::new();
    h.line("sync");
    h.frame(&["00000", "00000", "00300", "00000", "00000"]);
    h.advance(1000);
    h.frame(&["00000", "00000", "00100", "00040", "00000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2)), (W, p(3, 3))]);
    let viewed = h.b.main_end();
    let first = h.b.rec.parent(viewed).unwrap();
    h.b.move_to(first);
    h.ctx.float_edit_mode = true;
    h.frame(&["00000", "00000", "00100", "00020", "03000"]);
    assert_eq!(h.b.main_line(), [(B, p(2, 2)), (W, p(3, 3)), (B, p(1, 4))]);
    assert_eq!(h.b.cursor, first, "the view stays where the user is");
    h.ctx.float_edit_mode = false;
    h.frame(&["00000", "00000", "00100", "00020", "01004"]);
    assert_eq!(h.b.cursor, h.b.main_end(), "outside the edit mode the view follows the new move");
}
